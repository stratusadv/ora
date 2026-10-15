use crate::audio;
use crate::error::{AppError, AppResult};
use crate::settings;
use crate::state::AppState;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const EVENT_WAVEFORM: &str = "waveform";
const FRAME_MS: u64 = 33;
const POINT_PRECISION: f32 = 100.0;
const POINTS: u32 = 240;
const RING_MS: u32 = 120;
const STREAM_TIMEOUT_MS: u64 = 5000;
const TRIGGER_SEARCH_MS: u32 = 20;
const WAVEFORM_GAIN: f32 = 8.0;
const WINDOW_MS: u32 = 40;

const _: () = assert!(WINDOW_MS + TRIGGER_SEARCH_MS < RING_MS);
const _: () = assert!(POINTS >= 2);

#[derive(Debug, Default)]
pub(crate) struct MonitorState {
    stop: Option<mpsc::Sender<()>>,
    wanted: bool,
}

#[derive(Debug)]
struct Ring {
    cursor: usize,
    samples: Vec<f32>,
}

impl Ring {
    fn new(capacity: usize) -> Self {
        debug_assert!(capacity > 0);

        Self { cursor: 0, samples: vec![0.0; capacity] }
    }

    fn push(&mut self, sample: f32) {
        if let Some(slot) = self.samples.get_mut(self.cursor) {
            *slot = sample;
        }

        self.cursor = (self.cursor + 1) % self.samples.len();

        debug_assert!(self.cursor < self.samples.len());
    }
}

pub(crate) fn monitor_close(app: &AppHandle) {
    monitor_update(app, |state| {
        state.wanted = false;
        stream_stop(state);
    });
}

pub(crate) fn monitor_open(app: &AppHandle) {
    let microphone = microphone_current();
    let handle = app.clone();

    monitor_update(app, move |state| {
        state.wanted = true;
        stream_stop(state);
        state.stop = stream_spawn(handle, microphone);
    });
}

pub(crate) fn monitor_pause(app: &AppHandle) {
    monitor_update(app, stream_stop);
}

pub(crate) fn monitor_resume(app: &AppHandle) {
    let wanted = app.state::<AppState>().monitor.lock().is_ok_and(|state| state.wanted);

    if wanted {
        monitor_open(app);
    }
}

fn ring_ordered(samples: &[f32], cursor: usize) -> Vec<f32> {
    debug_assert!(cursor < samples.len().max(1));

    let (newer, older) = samples.split_at(cursor.min(samples.len()));
    let ordered: Vec<f32> = older.iter().chain(newer).copied().collect();

    debug_assert_eq!(ordered.len(), samples.len());

    ordered
}

fn crossing_upward(pair: &[f32]) -> bool {
    let [before, after] = pair else {
        return false;
    };

    if *before < 0.0 {
        return *after >= 0.0;
    }

    false
}

fn microphone_current() -> Option<String> {
    settings::root_resolve()
        .and_then(|root| settings::settings_load(&root))
        .map(|settings| settings.microphone)
        .unwrap_or_default()
}

fn monitor_update<Mutate>(app: &AppHandle, mutate: Mutate)
where
    Mutate: FnOnce(&mut MonitorState),
{
    let state = app.state::<AppState>();

    let Ok(mut guard) = state.monitor.lock() else {
        tracing::error!("the monitor state was poisoned");

        return;
    };

    mutate(&mut guard);
}

fn stream_build(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    ring: Arc<Mutex<Ring>>,
) -> AppResult<cpal::Stream> {
    let config = audio::stream_config_of(supported);

    match supported.sample_format() {
        SampleFormat::F32 => stream_typed::<f32>(device, config, ring),
        SampleFormat::F64 => stream_typed::<f64>(device, config, ring),
        SampleFormat::I8 => stream_typed::<i8>(device, config, ring),
        SampleFormat::I16 => stream_typed::<i16>(device, config, ring),
        SampleFormat::I32 => stream_typed::<i32>(device, config, ring),
        SampleFormat::U8 => stream_typed::<u8>(device, config, ring),
        SampleFormat::U16 => stream_typed::<u16>(device, config, ring),
        format => Err(AppError::Audio(format!(
            "the microphone delivers {format} samples, which ora does not read"
        ))),
    }
}

fn stream_error_log(error: &cpal::Error) {
    if error.kind() == cpal::ErrorKind::Xrun {
        tracing::debug!("the live signal stream lost a buffer: {error}");

        return;
    }

    tracing::warn!("the live signal stream reported an error: {error}");
}

fn stream_run(
    app: &AppHandle,
    microphone: Option<&str>,
    stop: &mpsc::Receiver<()>,
) -> AppResult<()> {
    let host = cpal::default_host();
    let device = audio::device_resolve(&host, microphone)?;
    let supported = audio::input_config_choose(&device)?;
    let sample_rate = supported.sample_rate();
    let capacity = (sample_rate * RING_MS).div_euclid(1000) as usize;
    let ring = Arc::new(Mutex::new(Ring::new(capacity.max(1))));
    let stream = stream_build(&device, &supported, Arc::clone(&ring))?;

    stream
        .play()
        .map_err(|error| AppError::Audio(format!("the microphone did not start: {error}")))?;

    while stop.recv_timeout(Duration::from_millis(FRAME_MS)) == Err(RecvTimeoutError::Timeout) {
        let ordered = match ring.lock() {
            Ok(ring) => ring_ordered(&ring.samples, ring.cursor),
            Err(_) => break,
        };

        let points = waveform_points(&ordered, sample_rate);

        if let Err(error) = app.emit(EVENT_WAVEFORM, points) {
            tracing::debug!("the waveform was not announced: {error}");
        }
    }

    drop(stream);

    Ok(())
}

fn stream_spawn(app: AppHandle, microphone: Option<String>) -> Option<mpsc::Sender<()>> {
    let (stop_sender, stop_receiver) = mpsc::channel();

    let spawned = thread::Builder::new().name("ora-monitor".into()).spawn(move || {
        if let Err(error) = stream_run(&app, microphone.as_deref(), &stop_receiver) {
            tracing::warn!("the live signal could not open the microphone: {error}");
        }
    });

    match spawned {
        Ok(_handle) => Some(stop_sender),
        Err(error) => {
            tracing::warn!("the monitor thread did not start: {error}");

            None
        }
    }
}

fn stream_stop(state: &mut MonitorState) {
    if let Some(stop) = state.stop.take() {
        if stop.send(()).is_err() {
            tracing::debug!("the live signal had already stopped");
        }
    }

    debug_assert!(state.stop.is_none());
}

fn stream_typed<T>(
    device: &cpal::Device,
    config: StreamConfig,
    ring: Arc<Mutex<Ring>>,
) -> AppResult<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels.max(1));
    let scale = 1.0 / f32::from(config.channels.max(1));

    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let Ok(mut ring) = ring.lock() else {
                    return;
                };

                for frame in data.chunks_exact(channels) {
                    let sum: f32 = frame.iter().map(|sample| sample.to_sample::<f32>()).sum();

                    ring.push(sum * scale);
                }
            },
            |error| stream_error_log(&error),
            Some(Duration::from_millis(STREAM_TIMEOUT_MS)),
        )
        .map_err(|error| AppError::Audio(format!("the microphone did not open: {error}")))
}

fn waveform_points(ordered: &[f32], sample_rate: u32) -> Vec<f32> {
    let window = (sample_rate * WINDOW_MS).div_euclid(1000) as usize;
    let search = (sample_rate * TRIGGER_SEARCH_MS).div_euclid(1000) as usize;
    let tail = ordered.get(ordered.len().saturating_sub(window + search)..).unwrap_or_default();

    let trigger =
        tail.windows(2).take(search).position(crossing_upward).map_or(0, |index| index + 1);

    let shown = tail.get(trigger..).unwrap_or_default();
    let shown = shown.get(..window.min(shown.len())).unwrap_or_default();
    let points = POINTS as usize;

    let values: Vec<f32> = (0..points)
        .map(|index| {
            let start = (index * shown.len()).div_euclid(points);
            let end = ((index + 1) * shown.len()).div_euclid(points).max(start + 1);
            let slice = shown.get(start..end.min(shown.len())).unwrap_or_default();
            let count = f32::from(u16::try_from(slice.len()).unwrap_or(u16::MAX).max(1));
            let mean = slice.iter().sum::<f32>() / count;

            ((mean * WAVEFORM_GAIN).tanh() * POINT_PRECISION).round() / POINT_PRECISION
        })
        .collect();

    debug_assert_eq!(values.len(), points);
    debug_assert!(values.iter().all(|value| (-1.0..=1.0).contains(value)));

    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: u32 = 16_000;
    const RATE_HZ: f32 = 16_000.0;

    fn sine(frequency: f32, amplitude: f32, phase: f32, count: u32) -> Vec<f32> {
        let step = TAU * frequency / RATE_HZ;

        std::iter::successors(Some(phase), |angle| Some(angle + step))
            .take(count as usize)
            .map(|angle| angle.sin() * amplitude)
            .collect()
    }

    fn peak(points: &[f32]) -> f32 {
        points.iter().map(|point| point.abs()).fold(0.0, f32::max)
    }

    #[test]
    fn a_ring_reads_back_the_newest_samples_oldest_first() {
        let mut ring = Ring::new(5);

        for value in (0_u8..12).map(f32::from) {
            ring.push(value);
        }

        assert_eq!(ring_ordered(&ring.samples, ring.cursor), vec![7.0, 8.0, 9.0, 10.0, 11.0]);
    }

    #[test]
    fn silence_draws_a_flat_line_of_the_full_width() {
        let silent = vec![0.0; (RATE * RING_MS).div_euclid(1000) as usize];
        let points = waveform_points(&silent, RATE);

        assert_eq!(points.len(), POINTS as usize);
        assert!(points.iter().all(|point| point.abs() < f32::EPSILON));
    }

    #[test]
    fn the_trigger_draws_the_same_tone_the_same_way_whatever_its_phase() {
        let count = (RATE * RING_MS).div_euclid(1000);
        let early = waveform_points(&sine(200.0, 0.2, 0.0, count), RATE);
        let late = waveform_points(&sine(200.0, 0.2, 2.1, count), RATE);

        let drift = early
            .iter()
            .zip(&late)
            .map(|(early_point, late_point)| (early_point - late_point).abs())
            .fold(0.0, f32::max);

        let rising = early.get(..4).unwrap_or_default();

        assert!(drift <= 0.1, "the triggered traces differ by up to {drift}");
        assert!(early.first().is_some_and(|first| *first >= 0.0));
        assert!(
            rising.windows(2).all(|pair| matches!(pair, [before, after] if after > before)),
            "{rising:?}"
        );
    }

    #[test]
    fn a_loud_signal_stays_inside_the_scale_and_a_quiet_one_still_shows() {
        let count = (RATE * RING_MS).div_euclid(1000);
        let loud = waveform_points(&sine(150.0, 1.0, 0.0, count), RATE);
        let quiet = waveform_points(&sine(150.0, 0.02, 0.0, count), RATE);
        let quiet_peak = peak(&quiet);

        assert!(peak(&loud) <= 1.0);
        assert!(quiet_peak >= 0.08, "a quiet voice peaked at only {quiet_peak}");
    }

    #[test]
    fn a_short_buffer_still_yields_every_point() {
        let points = waveform_points(&[0.1, -0.1, 0.1], RATE);

        assert_eq!(points.len(), POINTS as usize);
    }
}
