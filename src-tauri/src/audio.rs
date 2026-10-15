use crate::error::{AppError, AppResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize,
    FromSample,
    I24,
    Sample,
    SampleFormat,
    SizedSample,
    StreamConfig,
    SupportedBufferSize,
    U24,
};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async,
    FixedAsync,
    Resampler,
    SincInterpolationParameters,
    SincInterpolationType,
    WindowFunction,
};
use serde::Serialize;
use std::f32::consts::TAU;
use std::io::Cursor;
use std::str::FromStr;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub(crate) const SAMPLE_RATE_TARGET: u32 = 16_000;
const CAPTURE_SECONDS_INITIAL: u32 = 30;
const STREAM_BUFFER_MS: u32 = 20;
const CHANNELS_MAX: u16 = 32;
const CONFIG_COUNT_MAX: u32 = 1024;
const CUE_AMPLITUDE: f32 = 0.2;
const CUE_DURATION_MS_MAX: u32 = 300;
const CUE_FADE_MS: u32 = 8;
const CUE_MARGIN_MS: u64 = 150;
const DEVICE_COUNT_MAX: u32 = 64;
const DEVICE_MONITOR_SUFFIX: &str = ".monitor";
const MILLISECONDS_PER_SECOND: u32 = 1000;
const RECORDING_MS_MIN: u32 = 300;
pub(crate) const RECORDING_SECONDS_MAX: u32 = 300;
const RESAMPLE_CHUNK_FRAMES: u32 = 1024;
const SAMPLE_RATE_MAX: u32 = 192_000;
const SAMPLE_RATE_MIN: u32 = 8_000;
const SINC_LENGTH: u32 = 128;
const SINC_OVERSAMPLING: u32 = 128;
const SPEECH_FRAME_MS: u32 = 20;
const SPEECH_RMS_MIN: f32 = 0.01;
const STREAM_TIMEOUT_MS: u64 = 5000;
const TAIL_MS_MAX: u32 = 350;
const TAIL_MS_MIN: u32 = 40;
const TAIL_QUIET_MS: u32 = 80;
const TAIL_RMS_QUIET: f32 = 0.005;
const TAIL_STEP_MS: u32 = 20;
const WAV_BITS_PER_SAMPLE: u16 = 16;
const WAV_HEADER_BYTES: u32 = 64;
const WAV_SAMPLE_BYTES: u16 = WAV_BITS_PER_SAMPLE.div_euclid(8);

const _: () = assert!(SAMPLE_RATE_MIN <= SAMPLE_RATE_TARGET);
const _: () = assert!(SAMPLE_RATE_TARGET <= SAMPLE_RATE_MAX);
const _: () = assert!(SAMPLE_RATE_MAX * CUE_DURATION_MS_MAX < MILLISECONDS_PER_SECOND << 16);
const _: () = assert!(SAMPLE_RATE_MAX * SPEECH_FRAME_MS < MILLISECONDS_PER_SECOND << 16);
const _: () = assert!(2 * CUE_FADE_MS < CUE_DURATION_MS_MAX);
const _: () = assert!(RECORDING_MS_MIN < RECORDING_SECONDS_MAX * MILLISECONDS_PER_SECOND);
const _: () = assert!(SAMPLE_RATE_MIN * STREAM_BUFFER_MS >= MILLISECONDS_PER_SECOND);
const _: () = assert!(TAIL_MS_MIN < TAIL_MS_MAX);
const _: () = assert!(TAIL_STEP_MS <= TAIL_MS_MIN);
const _: () = assert!(TAIL_RMS_QUIET < SPEECH_RMS_MIN);
const _: () = assert!(SAMPLE_RATE_MAX * TAIL_QUIET_MS < MILLISECONDS_PER_SECOND << 16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cue {
    Error,
    Start,
    Stop,
}

impl Cue {
    const fn duration_ms(self) -> u32 {
        match self {
            Self::Error => 220,
            Self::Start | Self::Stop => 80,
        }
    }

    const fn frequency_hz(self) -> f32 {
        match self {
            Self::Error => 220.0,
            Self::Start => 880.0,
            Self::Stop => 660.0,
        }
    }
}

#[derive(Debug)]
struct InputSink {
    buffer: Arc<Mutex<Vec<f32>>>,
    samples_max: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Microphone {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Debug)]
pub(crate) struct Recording {
    pub(crate) sample_rate: u32,
    pub(crate) samples: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
struct Tone {
    frame: u16,
    frames_fade: u16,
    frames_total: u16,
    phase: f32,
    phase_step: f32,
}

impl Tone {
    fn new(cue: Cue, sample_rate: u32) -> Self {
        debug_assert!(cue.duration_ms() <= CUE_DURATION_MS_MAX);
        debug_assert!(sample_rate <= SAMPLE_RATE_MAX);

        let frames_total = frames_for(sample_rate, cue.duration_ms());
        let frames_fade = frames_for(sample_rate, CUE_FADE_MS);
        let phase_step = TAU * cue.frequency_hz() / sample_rate_f32(sample_rate);

        debug_assert!(frames_fade.saturating_mul(2) < frames_total);

        Self { frame: 0, frames_fade, frames_total, phase: 0.0, phase_step }
    }

    fn sample_next(&mut self) -> f32 {
        if self.frame >= self.frames_total {
            return 0.0;
        }

        let frames_left = self.frames_total - self.frame;
        let ramp = self.frame.min(frames_left).min(self.frames_fade);
        let envelope = f32::from(ramp) / f32::from(self.frames_fade.max(1));
        let value = self.phase.sin() * CUE_AMPLITUDE * envelope;
        self.frame += 1;
        self.phase = (self.phase + self.phase_step) % TAU;

        debug_assert!(value.abs() <= CUE_AMPLITUDE);

        value
    }
}

pub(crate) fn capture_spawn<Ended>(
    microphone: Option<String>,
    cues: bool,
    ended: Ended,
) -> AppResult<mpsc::Sender<()>>
where
    Ended: FnOnce(AppResult<Recording>) + Send + 'static,
{
    let (stop_sender, stop_receiver) = mpsc::channel();

    let handle = thread::Builder::new()
        .name("ora-capture".into())
        .spawn(move || ended(capture_run(microphone.as_deref(), cues, &stop_receiver)))
        .map_err(|error| AppError::Audio(format!("the capture thread did not start: {error}")))?;

    drop(handle);

    Ok(stop_sender)
}

pub(crate) fn cue_play(cue: Cue) {
    let spawned = thread::Builder::new().name("ora-cue".into()).spawn(move || {
        if let Err(error) = cue_run(cue) {
            tracing::warn!("the {cue:?} cue did not play: {error}");
        }
    });

    if let Err(error) = spawned {
        tracing::warn!("the cue thread did not start: {error}");
    }
}

pub(crate) fn microphones_list() -> AppResult<Vec<Microphone>> {
    let host = cpal::default_host();

    let devices = host.input_devices().map_err(|error| {
        AppError::Audio(format!("the microphones could not be listed: {error}"))
    })?;

    let mut microphones: Vec<Microphone> = devices
        .take(DEVICE_COUNT_MAX as usize)
        .filter_map(|device| microphone_describe(&device))
        .filter(|microphone| !microphone.id.ends_with(DEVICE_MONITOR_SUFFIX))
        .collect();

    microphones.sort_by(|left, right| left.name.cmp(&right.name));

    debug_assert!(microphones.len() <= DEVICE_COUNT_MAX as usize);

    Ok(microphones)
}

pub(crate) fn recording_encode(recording: &Recording) -> AppResult<Option<Vec<u8>>> {
    debug_assert!(recording.sample_rate >= SAMPLE_RATE_MIN);
    debug_assert!(recording.sample_rate <= SAMPLE_RATE_MAX);

    if !speech_detect(&recording.samples, recording.sample_rate) {
        return Ok(None);
    }

    if recording.sample_rate == SAMPLE_RATE_TARGET {
        return wav_encode(&recording.samples).map(Some);
    }

    let samples = resample(&recording.samples, recording.sample_rate)?;
    let wav = wav_encode(&samples)?;

    debug_assert!(wav.len() > samples.len());

    Ok(Some(wav))
}

fn capture_run(
    microphone: Option<&str>,
    cues: bool,
    stop: &mpsc::Receiver<()>,
) -> AppResult<Recording> {
    let host = cpal::default_host();
    let device = device_resolve(&host, microphone)?;

    let supported = input_config_choose(&device)?;
    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    let described = microphone_describe(&device);
    let name = described.map_or_else(|| "an unnamed device".into(), |microphone| microphone.name);

    tracing::info!("recording from {name} at {sample_rate} Hz, {channels} channel(s)");

    if !(SAMPLE_RATE_MIN..=SAMPLE_RATE_MAX).contains(&sample_rate) {
        return Err(AppError::Audio(format!(
            "the microphone runs at {sample_rate} Hz, outside the range ora records"
        )));
    }

    if !(1..=CHANNELS_MAX).contains(&channels) {
        return Err(AppError::Audio(format!(
            "the microphone reports {channels} channels, outside the range ora records"
        )));
    }

    let samples_max = sample_rate * RECORDING_SECONDS_MAX;
    let samples_initial = sample_rate as usize * CAPTURE_SECONDS_INITIAL as usize;
    let buffer = Arc::new(Mutex::new(Vec::with_capacity(samples_initial)));
    let stream = stream_input_build(&device, &supported, Arc::clone(&buffer), samples_max)?;

    stream
        .play()
        .map_err(|error| AppError::Audio(format!("the microphone did not start: {error}")))?;

    if cues {
        cue_play(Cue::Start);
    }

    let waited = stop.recv_timeout(Duration::from_secs(u64::from(RECORDING_SECONDS_MAX)));

    if matches!(waited, Err(RecvTimeoutError::Timeout)) {
        tracing::info!("the recording reached its limit of {RECORDING_SECONDS_MAX} seconds");
    } else {
        tail_wait(&buffer, sample_rate);
    }

    drop(stream);

    if cues {
        cue_play(Cue::Stop);
    }

    let samples = buffer
        .lock()
        .map(|mut samples| std::mem::take(&mut *samples))
        .map_err(|error| AppError::Audio(format!("the recording buffer was poisoned: {error}")))?;

    debug_assert!(samples.len() <= samples_max as usize);

    Ok(Recording { sample_rate, samples })
}

fn cue_run(cue: Cue) -> AppResult<()> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or_else(|| AppError::Audio("no speaker or headset is connected".into()))?;

    let supported = device
        .default_output_config()
        .map_err(|error| AppError::Audio(format!("the speaker reported no format: {error}")))?;

    let stream = stream_output_build(&device, &supported, cue)?;

    stream
        .play()
        .map_err(|error| AppError::Audio(format!("the speaker did not start: {error}")))?;

    thread::sleep(Duration::from_millis(u64::from(cue.duration_ms()) + CUE_MARGIN_MS));

    drop(stream);

    Ok(())
}

fn decibels_of(amplitude: f32) -> f32 {
    let decibels = 20.0 * amplitude.max(1e-6).log10();

    debug_assert!(decibels >= -120.0);

    decibels
}

pub(crate) fn device_resolve(
    host: &cpal::Host,
    microphone: Option<&str>,
) -> AppResult<cpal::Device> {
    if let Some(id_text) = microphone {
        let found = cpal::DeviceId::from_str(id_text).ok().and_then(|id| host.device_by_id(&id));

        if let Some(device) = found {
            return Ok(device);
        }

        tracing::warn!("the microphone {id_text} is not connected, using the system default");
    }

    host.default_input_device().ok_or_else(|| AppError::Audio("no microphone is connected".into()))
}

fn frames_append<T>(buffer: &Mutex<Vec<f32>>, data: &[T], channels: u16, samples_max: u32)
where
    T: Sample,
    f32: FromSample<T>,
{
    debug_assert!(channels >= 1);

    let Ok(mut samples) = buffer.lock() else {
        return;
    };

    let room = (samples_max as usize).saturating_sub(samples.len());
    let scale = 1.0 / f32::from(channels);

    for frame in data.chunks_exact(usize::from(channels)).take(room) {
        let sum: f32 = frame.iter().map(|sample| sample.to_sample::<f32>()).sum();

        samples.push(sum * scale);
    }

    debug_assert!(samples.len() <= samples_max as usize);
}

fn frames_for(sample_rate: u32, duration_ms: u32) -> u16 {
    let frames = (sample_rate * duration_ms).div_euclid(MILLISECONDS_PER_SECOND);

    debug_assert!(frames < 1 << 16);

    u16::try_from(frames).unwrap_or(u16::MAX)
}

pub(crate) fn input_config_choose(device: &cpal::Device) -> AppResult<cpal::SupportedStreamConfig> {
    let default = device
        .default_input_config()
        .map_err(|error| AppError::Audio(format!("the microphone reported no format: {error}")))?;

    let ranges: Vec<cpal::SupportedStreamConfigRange> = device
        .supported_input_configs()
        .map(|ranges| ranges.take(CONFIG_COUNT_MAX as usize).collect())
        .unwrap_or_default();

    Ok(input_config_matching(&ranges, &default).unwrap_or(default))
}

fn input_config_matching(
    ranges: &[cpal::SupportedStreamConfigRange],
    default: &cpal::SupportedStreamConfig,
) -> Option<cpal::SupportedStreamConfig> {
    let config = ranges
        .iter()
        .filter(|range| range.sample_format() == default.sample_format())
        .filter(|range| range.channels() == default.channels())
        .find_map(|range| range.try_with_sample_rate(SAMPLE_RATE_TARGET))?;

    debug_assert_eq!(config.sample_format(), default.sample_format());
    debug_assert_eq!(config.sample_rate(), SAMPLE_RATE_TARGET);

    Some(config)
}

fn microphone_describe(device: &cpal::Device) -> Option<Microphone> {
    let id = device.id().ok()?.to_string();

    let name = device
        .description()
        .map_or_else(|_| id.clone(), |description| description.name().to_owned());

    Some(Microphone { id, name })
}

fn resample(samples: &[f32], sample_rate: u32) -> AppResult<Vec<f32>> {
    if sample_rate == SAMPLE_RATE_TARGET {
        return Ok(samples.to_vec());
    }

    let ratio = f64::from(SAMPLE_RATE_TARGET) / f64::from(sample_rate);

    let parameters =
        SincInterpolationParameters::new(SINC_LENGTH as usize, WindowFunction::BlackmanHarris2)
            .oversampling_factor(SINC_OVERSAMPLING as usize)
            .interpolation(SincInterpolationType::Cubic);

    let mut resampler = Async::<f32>::new_sinc(
        ratio,
        1.0,
        &parameters,
        RESAMPLE_CHUNK_FRAMES as usize,
        1,
        FixedAsync::Input,
    )
    .map_err(|error| AppError::Audio(format!("the resampler did not start: {error}")))?;

    let input = InterleavedSlice::new(samples, 1, samples.len())
        .map_err(|error| AppError::Audio(format!("the resampler refused the audio: {error}")))?;

    let output = resampler
        .process_all(&input, samples.len(), None)
        .map_err(|error| AppError::Audio(format!("the audio did not resample: {error}")))?
        .take_data();

    debug_assert!(output.len() <= samples.len());

    Ok(output)
}

pub(crate) fn recording_levels(recording: &Recording) -> (f32, f32) {
    let frame_samples = usize::from(frames_for(recording.sample_rate, SPEECH_FRAME_MS));
    let peak = recording.samples.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()));

    let loudest = recording.samples.chunks_exact(frame_samples).map(rms).fold(0.0_f32, f32::max);

    debug_assert!(loudest <= peak + f32::EPSILON);

    (decibels_of(peak), decibels_of(loudest))
}

fn rms(frame: &[f32]) -> f32 {
    debug_assert_ne!(frame.len(), 0);

    let energy: f32 = frame.iter().map(|sample| sample * sample).sum();
    let count = u16::try_from(frame.len()).map_or_else(|_| f32::from(u16::MAX), f32::from);
    let level = (energy / count).sqrt();

    debug_assert!(level >= 0.0);

    level
}

fn sample_rate_f32(sample_rate: u32) -> f32 {
    debug_assert!(sample_rate <= SAMPLE_RATE_MAX);

    let high = u16::try_from(sample_rate >> 16).unwrap_or(u16::MAX);
    let low = u16::try_from(sample_rate & 0xFFFF).unwrap_or(u16::MAX);
    let converted = f32::from(high).mul_add(65_536.0, f32::from(low));

    debug_assert!(converted >= 0.0);

    converted
}

fn speech_detect(samples: &[f32], sample_rate: u32) -> bool {
    let samples_min = (sample_rate * RECORDING_MS_MIN).div_euclid(MILLISECONDS_PER_SECOND);
    let frame_samples = frames_for(sample_rate, SPEECH_FRAME_MS);

    debug_assert!(frame_samples > 0);

    if samples.len() < samples_min as usize {
        return false;
    }

    samples.chunks_exact(usize::from(frame_samples)).any(|frame| rms(frame) >= SPEECH_RMS_MIN)
}

pub(crate) fn stream_config_of(supported: &cpal::SupportedStreamConfig) -> StreamConfig {
    let mut config = supported.config();
    let frames = (supported.sample_rate() * STREAM_BUFFER_MS).div_euclid(MILLISECONDS_PER_SECOND);

    debug_assert!(frames > 0);

    config.buffer_size = match supported.buffer_size() {
        SupportedBufferSize::Range { min, max } => BufferSize::Fixed(frames.clamp(*min, *max)),
        SupportedBufferSize::Unknown => BufferSize::Fixed(frames),
    };

    config
}

fn stream_input_build(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    samples_max: u32,
) -> AppResult<cpal::Stream> {
    let config = stream_config_of(supported);
    let sink = InputSink { buffer, samples_max };

    match supported.sample_format() {
        SampleFormat::F32 => stream_input_typed::<f32>(device, config, sink),
        SampleFormat::F64 => stream_input_typed::<f64>(device, config, sink),
        SampleFormat::I8 => stream_input_typed::<i8>(device, config, sink),
        SampleFormat::I16 => stream_input_typed::<i16>(device, config, sink),
        SampleFormat::I24 => stream_input_typed::<I24>(device, config, sink),
        SampleFormat::I32 => stream_input_typed::<i32>(device, config, sink),
        SampleFormat::I64 => stream_input_typed::<i64>(device, config, sink),
        SampleFormat::U8 => stream_input_typed::<u8>(device, config, sink),
        SampleFormat::U16 => stream_input_typed::<u16>(device, config, sink),
        SampleFormat::U24 => stream_input_typed::<U24>(device, config, sink),
        SampleFormat::U32 => stream_input_typed::<u32>(device, config, sink),
        SampleFormat::U64 => stream_input_typed::<u64>(device, config, sink),
        format => Err(AppError::Audio(format!(
            "the microphone delivers {format} samples, which ora does not read"
        ))),
    }
}

fn stream_input_typed<T>(
    device: &cpal::Device,
    config: StreamConfig,
    sink: InputSink,
) -> AppResult<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels;

    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                frames_append(&sink.buffer, data, channels, sink.samples_max);
            },
            |error| tracing::warn!("the microphone stream reported an error: {error}"),
            Some(Duration::from_millis(STREAM_TIMEOUT_MS)),
        )
        .map_err(|error| AppError::Audio(format!("the microphone did not open: {error}")))
}

fn stream_output_build(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    cue: Cue,
) -> AppResult<cpal::Stream> {
    let config = stream_config_of(supported);

    match supported.sample_format() {
        SampleFormat::F32 => stream_output_typed::<f32>(device, config, cue),
        SampleFormat::F64 => stream_output_typed::<f64>(device, config, cue),
        SampleFormat::I8 => stream_output_typed::<i8>(device, config, cue),
        SampleFormat::I16 => stream_output_typed::<i16>(device, config, cue),
        SampleFormat::I24 => stream_output_typed::<I24>(device, config, cue),
        SampleFormat::I32 => stream_output_typed::<i32>(device, config, cue),
        SampleFormat::I64 => stream_output_typed::<i64>(device, config, cue),
        SampleFormat::U8 => stream_output_typed::<u8>(device, config, cue),
        SampleFormat::U16 => stream_output_typed::<u16>(device, config, cue),
        SampleFormat::U24 => stream_output_typed::<U24>(device, config, cue),
        SampleFormat::U32 => stream_output_typed::<u32>(device, config, cue),
        SampleFormat::U64 => stream_output_typed::<u64>(device, config, cue),
        format => Err(AppError::Audio(format!(
            "the speaker takes {format} samples, which ora does not write"
        ))),
    }
}

fn stream_output_typed<T>(
    device: &cpal::Device,
    config: StreamConfig,
    cue: Cue,
) -> AppResult<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels;
    let mut tone = Tone::new(cue, config.sample_rate);

    debug_assert!(channels >= 1);

    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                tone_fill(&mut tone, data, channels);
            },
            |error| tracing::warn!("the speaker stream reported an error: {error}"),
            Some(Duration::from_millis(STREAM_TIMEOUT_MS)),
        )
        .map_err(|error| AppError::Audio(format!("the speaker did not open: {error}")))
}

fn tail_quiet(samples: &[f32], sample_rate: u32) -> bool {
    let window = usize::from(frames_for(sample_rate, TAIL_QUIET_MS));

    debug_assert!(window > 0);

    if samples.len() < window {
        return false;
    }

    samples.get(samples.len() - window..).is_some_and(|tail| rms(tail) < TAIL_RMS_QUIET)
}

fn tail_wait(buffer: &Mutex<Vec<f32>>, sample_rate: u32) {
    let steps_min = TAIL_MS_MIN.div_euclid(TAIL_STEP_MS);
    let steps_max = TAIL_MS_MAX.div_euclid(TAIL_STEP_MS);

    for step in 1..=steps_max {
        thread::sleep(Duration::from_millis(u64::from(TAIL_STEP_MS)));

        if step < steps_min {
            continue;
        }

        if buffer.lock().is_ok_and(|samples| tail_quiet(&samples, sample_rate)) {
            return;
        }
    }
}

fn tone_fill<T>(tone: &mut Tone, data: &mut [T], channels: u16)
where
    T: Sample + FromSample<f32>,
{
    debug_assert!(channels >= 1);

    for frame in data.chunks_exact_mut(usize::from(channels)) {
        frame.fill(tone.sample_next().to_sample::<T>());
    }
}

fn wav_encode(samples: &[f32]) -> AppResult<Vec<u8>> {
    let capacity = samples.len() * usize::from(WAV_SAMPLE_BYTES) + WAV_HEADER_BYTES as usize;
    let mut cursor = Cursor::new(Vec::with_capacity(capacity));

    {
        let specification = hound::WavSpec {
            bits_per_sample: WAV_BITS_PER_SAMPLE,
            channels: 1,
            sample_format: hound::SampleFormat::Int,
            sample_rate: SAMPLE_RATE_TARGET,
        };

        let mut writer = hound::WavWriter::new(&mut cursor, specification)
            .map_err(|error| AppError::Audio(format!("the audio did not encode: {error}")))?;

        for sample in samples {
            writer
                .write_sample(sample.to_sample::<i16>())
                .map_err(|error| AppError::Audio(format!("the audio did not encode: {error}")))?;
        }

        writer
            .finalize()
            .map_err(|error| AppError::Audio(format!("the audio did not encode: {error}")))?;
    }

    let bytes = cursor.into_inner();

    debug_assert!(bytes.starts_with(b"RIFF"));
    debug_assert!(bytes.len() >= samples.len() * usize::from(WAV_SAMPLE_BYTES));

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frequency_hz: f32, sample_rate: u32, duration_ms: u32, amplitude: f32) -> Vec<f32> {
        let count = (sample_rate * duration_ms).div_euclid(MILLISECONDS_PER_SECOND);
        let step = TAU * frequency_hz / sample_rate_f32(sample_rate);
        let mut phase = 0.0_f32;

        (0..count)
            .map(|_| {
                let value = phase.sin() * amplitude;
                phase = (phase + step) % TAU;

                value
            })
            .collect()
    }

    fn crossings(samples: &[f32]) -> usize {
        samples.windows(2).filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0).count()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
    }

    fn config_of(sample_rate: u32, buffer_size: SupportedBufferSize) -> BufferSize {
        let supported =
            cpal::SupportedStreamConfig::new(1, sample_rate, buffer_size, SampleFormat::F32);

        stream_config_of(&supported).buffer_size
    }

    fn range(channels: u16, format: SampleFormat) -> cpal::SupportedStreamConfigRange {
        cpal::SupportedStreamConfigRange::new(
            channels,
            1,
            768_000,
            SupportedBufferSize::Range { min: 1, max: 1 << 20 },
            format,
        )
    }

    fn scaled<T>(data: &[T]) -> Vec<f32>
    where
        T: Sample,
        f32: FromSample<T>,
    {
        let buffer = Mutex::new(Vec::new());

        frames_append(&buffer, data, 1, 16);

        buffer.into_inner().unwrap()
    }

    #[test]
    fn frames_mix_down_to_mono_and_stop_at_the_cap() {
        let buffer = Mutex::new(Vec::new());
        let stereo = [1.0_f32, 0.0, 0.5, 0.5, -1.0, -1.0];

        frames_append(&buffer, &stereo, 2, 2);

        let samples = buffer.into_inner().unwrap();

        assert_eq!(samples.as_slice(), &[0.5, 0.5]);
    }

    #[test]
    fn integer_frames_are_scaled_into_the_unit_range() {
        let buffer = Mutex::new(Vec::new());

        frames_append(&buffer, &[i16::MAX, i16::MIN, 0], 1, 16);

        let samples = buffer.into_inner().unwrap();

        assert!((samples[0] - 1.0).abs() < 1e-3);
        assert!((samples[1] + 1.0).abs() < 1e-3);
        assert!(samples[2].abs() < f32::EPSILON);
    }

    #[test]
    fn frames_of_every_width_and_signedness_reach_the_same_unit_range() {
        let expected = [1.0_f32, -1.0, 0.0];
        let i24_max = I24::new((1 << 23) - 1).unwrap();
        let i24_min = I24::new(-(1 << 23)).unwrap();
        let u24_max = U24::new((1 << 24) - 1).unwrap();
        let u24_min = U24::new(0).unwrap();

        let results = [
            scaled(&[i24_max, i24_min, I24::EQUILIBRIUM]),
            scaled(&[u24_max, u24_min, U24::EQUILIBRIUM]),
            scaled(&[i64::MAX, i64::MIN, 0]),
            scaled(&[u32::MAX, u32::MIN, u32::EQUILIBRIUM]),
            scaled(&[u64::MAX, u64::MIN, u64::EQUILIBRIUM]),
        ];

        for samples in results {
            assert_eq!(samples.len(), expected.len());

            for (sample, wanted) in samples.iter().zip(expected) {
                assert!((sample - wanted).abs() < 1e-3, "{sample} is not {wanted}");
            }
        }
    }

    #[test]
    fn a_cue_written_as_24_bit_samples_reads_back_as_the_same_tone() {
        let mut tone_float = Tone::new(Cue::Start, 48_000);
        let mut tone_wide = Tone::new(Cue::Start, 48_000);
        let mut float = vec![0.0_f32; 960];
        let mut wide = vec![I24::EQUILIBRIUM; 960];

        tone_fill(&mut tone_float, &mut float, 2);
        tone_fill(&mut tone_wide, &mut wide, 2);

        let step = 1.0 / f32::from(1_u16 << 15) / 256.0;

        assert!(peak(&float) > 0.0);

        for (written, read) in float.iter().zip(&wide) {
            assert!((written - read.to_sample::<f32>()).abs() <= step);
        }
    }

    #[test]
    fn speech_needs_both_enough_length_and_enough_energy() {
        let silence = vec![0.0_f32; 48_000];
        let hum = sine(200.0, 48_000, 1000, 0.005);
        let voice = sine(200.0, 48_000, 1000, 0.1);
        let blip = sine(200.0, 48_000, 100, 0.5);

        assert!(!speech_detect(&silence, 48_000));
        assert!(!speech_detect(&hum, 48_000));
        assert!(speech_detect(&voice, 48_000));
        assert!(!speech_detect(&blip, 48_000));
    }

    #[test]
    fn resampling_keeps_the_pitch_and_length_and_removes_what_cannot_be_represented() {
        let voice = sine(440.0, 48_000, 2000, 0.5);
        let squeal = sine(12_000.0, 48_000, 2000, 0.5);
        let voice_resampled = resample(&voice, 48_000).unwrap();
        let squeal_resampled = resample(&squeal, 48_000).unwrap();
        let middle = 4_000..28_000;

        assert!(voice_resampled.len().abs_diff(voice.len().div_euclid(3)) <= 1);
        assert!(crossings(&voice_resampled[middle.clone()]).abs_diff(660) <= 2);
        assert!((peak(&voice_resampled[middle.clone()]) - 0.5).abs() < 0.02);
        assert!(peak(&squeal_resampled[middle]) < 0.01);
    }

    #[test]
    fn audio_at_the_target_rate_passes_through_untouched() {
        let voice = sine(440.0, SAMPLE_RATE_TARGET, 500, 0.5);

        assert_eq!(resample(&voice, SAMPLE_RATE_TARGET).unwrap(), voice);
    }

    #[test]
    fn an_encoded_wav_reads_back_as_the_same_mono_16_bit_samples() {
        let samples = [0.0_f32, 0.5, -0.5, 1.0, -1.0, 2.0];
        let bytes = wav_encode(&samples).unwrap();
        let mut reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        let specification = reader.spec();
        let decoded: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();

        assert_eq!(specification.channels, 1);
        assert_eq!(specification.sample_rate, SAMPLE_RATE_TARGET);
        assert_eq!(specification.bits_per_sample, 16);
        assert_eq!(decoded.len(), samples.len());
        assert_eq!(decoded[0], 0);
        assert!((i32::from(decoded[1]) - 16_384).abs() <= 1);
        assert_eq!(decoded[3], i16::MAX);
        assert_eq!(decoded[4], i16::MIN);
        assert_eq!(decoded[5], i16::MAX);
    }

    #[test]
    fn a_silent_or_short_recording_encodes_to_nothing() {
        let silent = Recording { sample_rate: 48_000, samples: vec![0.0; 48_000] };
        let spoken = Recording { sample_rate: 48_000, samples: sine(220.0, 48_000, 1000, 0.2) };

        assert!(recording_encode(&silent).unwrap().is_none());
        assert!(recording_encode(&spoken).unwrap().unwrap().starts_with(b"RIFF"));
    }

    #[test]
    fn a_tone_fades_in_and_out_and_stays_silent_after_its_end() {
        let mut tone = Tone::new(Cue::Start, 48_000);
        let total = usize::from(tone.frames_total);
        let values: Vec<f32> = (0..total + 100).map(|_| tone.sample_next()).collect();

        assert!(values[0].abs() < f32::EPSILON);
        assert!(values[total - 1].abs() < 0.01);
        assert!(peak(&values) <= CUE_AMPLITUDE);
        assert!(peak(&values) > CUE_AMPLITUDE * 0.9);
        assert!(values[total..].iter().all(|value| value.abs() < f32::EPSILON));
        assert!(crossings(&values[..total]).abs_diff(70) <= 1);
    }

    #[test]
    fn a_stream_asks_for_a_short_buffer_within_what_the_device_allows() {
        let wide = SupportedBufferSize::Range { min: 1, max: 1 << 20 };

        assert_eq!(config_of(48_000, wide), BufferSize::Fixed(960));
        assert_eq!(config_of(16_000, wide), BufferSize::Fixed(320));
        assert_eq!(config_of(44_100, SupportedBufferSize::Unknown), BufferSize::Fixed(882));
        assert_eq!(
            config_of(48_000, SupportedBufferSize::Range { min: 2048, max: 4096 }),
            BufferSize::Fixed(2048)
        );
        assert_eq!(
            config_of(48_000, SupportedBufferSize::Range { min: 64, max: 512 }),
            BufferSize::Fixed(512)
        );
    }

    #[test]
    fn the_tail_ends_only_after_enough_quiet_audio() {
        let mut samples = sine(200.0, 16_000, 500, 0.2);

        assert!(!tail_quiet(&samples, 16_000));

        samples.extend(vec![0.001_f32; 16 * 40]);

        assert!(!tail_quiet(&samples, 16_000));

        samples.extend(vec![0.001_f32; 16 * 40]);

        assert!(tail_quiet(&samples, 16_000));
        assert!(!tail_quiet(&samples[..100], 16_000));

        samples.extend(sine(200.0, 16_000, 20, 0.2));

        assert!(!tail_quiet(&samples, 16_000));
    }

    #[test]
    fn a_soft_trailing_sound_is_not_mistaken_for_quiet() {
        let breath = sine(4_000.0, 16_000, 200, 0.02);

        assert!(!tail_quiet(&breath, 16_000));
        assert!(rms(&breath) > TAIL_RMS_QUIET);
    }

    #[test]
    fn a_recording_at_the_target_rate_encodes_without_resampling() {
        let samples = sine(220.0, SAMPLE_RATE_TARGET, 1000, 0.2);
        let recording = Recording { sample_rate: SAMPLE_RATE_TARGET, samples: samples.clone() };
        let wav = recording_encode(&recording).unwrap().unwrap();

        assert_eq!(wav, wav_encode(&samples).unwrap());
    }

    #[test]
    fn levels_report_the_peak_and_the_loudest_stretch_in_decibels() {
        let half = Recording { sample_rate: 16_000, samples: sine(440.0, 16_000, 500, 0.5) };
        let (peak, loudest) = recording_levels(&half);

        assert!((peak - -6.02).abs() < 0.1);
        assert!((loudest - -9.03).abs() < 0.2);

        let silent = Recording { sample_rate: 16_000, samples: vec![0.0; 16_000] };

        assert_eq!(recording_levels(&silent), (-120.0, -120.0));
    }

    #[test]
    fn the_target_rate_keeps_the_devices_own_format_even_when_8_bit_is_listed_first() {
        let default = cpal::SupportedStreamConfig::new(
            1,
            48_000,
            SupportedBufferSize::Range { min: 1, max: 1 << 20 },
            SampleFormat::F32,
        );

        let ranges = [
            range(1, SampleFormat::U8),
            range(2, SampleFormat::F32),
            range(1, SampleFormat::I16),
            range(1, SampleFormat::F32),
        ];

        let chosen = input_config_matching(&ranges, &default).unwrap();

        assert_eq!(chosen.sample_format(), SampleFormat::F32);
        assert_eq!(chosen.channels(), 1);
        assert_eq!(chosen.sample_rate(), SAMPLE_RATE_TARGET);
        assert!(input_config_matching(&ranges[..3], &default).is_none());
    }

    #[test]
    fn a_sample_rate_converts_to_a_float_exactly() {
        assert!((sample_rate_f32(44_100) - 44_100.0).abs() < f32::EPSILON);
        assert!((sample_rate_f32(SAMPLE_RATE_MAX) - 192_000.0).abs() < f32::EPSILON);
        assert!((sample_rate_f32(SAMPLE_RATE_MIN) - 8_000.0).abs() < f32::EPSILON);
    }

    #[test]
    fn every_cue_fits_its_bounds_at_every_supported_rate() {
        for cue in [Cue::Error, Cue::Start, Cue::Stop] {
            for sample_rate in [SAMPLE_RATE_MIN, 44_100, 48_000, SAMPLE_RATE_MAX] {
                let tone = Tone::new(cue, sample_rate);

                assert!(tone.frames_total > 2 * tone.frames_fade);
                assert!(u32::from(tone.frames_total) < 1 << 16);
            }
        }
    }
}
