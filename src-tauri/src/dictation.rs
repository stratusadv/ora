use crate::audio::{self, Cue, Recording};
use crate::error::{AppError, AppResult};
use crate::keystroke;
use crate::service::{self, Endpoint, KeyPurpose};
use crate::settings::{self, Settings, ShortcutMode};
use crate::state::AppState;
use crate::tray;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::mpsc;

pub(crate) const EVENT_STATUS: &str = "status";
const COMMAND_QUEUE_CAPACITY: u32 = 16;
const HOLD_MS_MIN: u64 = 400;
const JOB_QUEUE_CAPACITY: u32 = 4;
const NOTICE_COPIED: &str = "The text was copied to the clipboard because this desktop blocks \
                             simulated input. Paste it with Ctrl+V.";
const NOTICE_COPIED_FOCUSED: &str = "The text was copied to the clipboard. Paste it with Ctrl+V.";
const NOTICE_SILENT: &str = "No speech was detected. Check the input device.";

const _: () = assert!(JOB_QUEUE_CAPACITY < COMMAND_QUEUE_CAPACITY);

#[derive(Debug)]
pub(crate) enum Command {
    CaptureEnded { generation: u32, result: AppResult<Recording> },
    ShortcutPressed,
    ShortcutReleased,
    Toggled,
}

#[derive(Debug, Clone)]
pub(crate) struct Dictation {
    commands: mpsc::Sender<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Idle,
    Listening,
    Transcribing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Status {
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) phase: Phase,
}

#[derive(Debug, Default)]
pub(crate) struct StatusState {
    error: Option<String>,
    jobs: u32,
    listening: bool,
    notice: Option<String>,
}

#[derive(Debug)]
struct Controller {
    commands: mpsc::Sender<Command>,
    generation: u32,
    jobs: mpsc::Sender<Recording>,
    listening: Option<Listening>,
}

#[derive(Debug)]
struct Delivery {
    notice: Option<String>,
    text: Option<String>,
}

#[derive(Debug)]
struct Listening {
    generation: u32,
    mode: ShortcutMode,
    started: Instant,
    stop: std::sync::mpsc::Sender<()>,
}

impl Controller {
    fn capture_ended(&mut self, app: &AppHandle, generation: u32, result: AppResult<Recording>) {
        let current =
            self.listening.as_ref().is_some_and(|listening| listening.generation == generation);

        if current {
            self.listening_stop(app);
        }

        match result {
            Ok(recording) => self.recording_enqueue(app, recording),
            Err(error) => error_report(app, &error),
        }

        if current {
            debug_assert!(self.listening.is_none());
        }
    }

    fn listening_start(&mut self, app: &AppHandle) {
        debug_assert!(self.listening.is_none());

        let settings = settings_current();

        let endpoint = match endpoint_check(&settings, KeyPurpose::Transcription) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                error_report(app, &error);
                tray::window_main_show(app);

                return;
            }
        };

        tauri::async_runtime::spawn(async move { service::connection_warm(&endpoint).await });

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let commands = self.commands.clone();

        let microphone = settings.microphone.clone();

        let spawned = audio::capture_spawn(microphone, settings.cues_enabled, move |result| {
            let ended = Command::CaptureEnded { generation, result };

            if let Err(error) = commands.blocking_send(ended) {
                tracing::error!("a finished recording could not reach the controller: {error}");
            }
        });

        match spawned {
            Ok(stop) => {
                let started = Instant::now();
                let mode = settings.shortcut_mode;
                self.listening = Some(Listening { generation, mode, started, stop });

                status_update(app, |status| {
                    status.error = None;
                    status.listening = true;
                    status.notice = None;
                });
            }
            Err(error) => error_report(app, &error),
        }
    }

    fn listening_stop(&mut self, app: &AppHandle) {
        let Some(listening) = self.listening.take() else {
            return;
        };

        if listening.stop.send(()).is_err() {
            tracing::debug!("the capture had already ended before it was stopped");
        }

        tracing::info!("listened for {} ms", listening.started.elapsed().as_millis());

        status_update(app, |status| status.listening = false);

        debug_assert!(self.listening.is_none());
    }

    fn recording_enqueue(&self, app: &AppHandle, recording: Recording) {
        status_update(app, |status| status.jobs += 1);

        if self.jobs.try_send(recording).is_ok() {
            return;
        }

        status_update(app, |status| status.jobs = status.jobs.saturating_sub(1));

        let error = AppError::Task(
            "the transcription queue is full, so this recording was discarded".into(),
        );

        error_report(app, &error);
    }

    fn shortcut_pressed(&mut self, app: &AppHandle) {
        let Some(listening) = self.listening.as_ref() else {
            self.listening_start(app);

            return;
        };

        if press_stops(listening.mode) {
            self.listening_stop(app);
        }
    }

    fn shortcut_released(&mut self, app: &AppHandle) {
        let Some(listening) = self.listening.as_ref() else {
            return;
        };

        if release_stops(listening.mode, listening.started.elapsed()) {
            self.listening_stop(app);
        }
    }

    fn step(&mut self, app: &AppHandle, command: Command) {
        match command {
            Command::CaptureEnded { generation, result } => {
                self.capture_ended(app, generation, result);
            }
            Command::ShortcutPressed => self.shortcut_pressed(app),
            Command::ShortcutReleased => self.shortcut_released(app),
            Command::Toggled => self.toggled(app),
        }
    }

    fn toggled(&mut self, app: &AppHandle) {
        if self.listening.is_some() {
            self.listening_stop(app);

            return;
        }

        self.listening_start(app);
    }
}

pub(crate) fn command_send(app: &AppHandle, command: Command) {
    let state = app.state::<AppState>();

    let Some(dictation) = state.dictation.get() else {
        tracing::warn!("a command arrived before dictation started: {command:?}");

        return;
    };

    if let Err(error) = dictation.commands.try_send(command) {
        tracing::warn!("a command was dropped because the controller is behind: {error}");
    }
}

pub(crate) fn dictation_start(app: &AppHandle) -> Dictation {
    let (commands, command_receiver) = mpsc::channel(COMMAND_QUEUE_CAPACITY as usize);
    let (jobs, job_receiver) = mpsc::channel(JOB_QUEUE_CAPACITY as usize);

    let controller =
        Controller { commands: commands.clone(), generation: 0, jobs, listening: None };

    tauri::async_runtime::spawn(controller_run(app.clone(), controller, command_receiver));
    tauri::async_runtime::spawn(worker_run(app.clone(), job_receiver));

    debug_assert_eq!(commands.max_capacity(), COMMAND_QUEUE_CAPACITY as usize);

    Dictation { commands }
}

pub(crate) fn error_report(app: &AppHandle, error: &AppError) {
    let detail = error.detail();

    tracing::error!("{error}");
    cue_play_configured(Cue::Error);
    status_update(app, |status| status.error = Some(detail.clone()));

    let shown = app.notification().builder().title(error.title()).body(&detail).show();

    if let Err(notification_error) = shown {
        tracing::warn!("the error notification was not shown: {notification_error}");
    }
}

pub(crate) fn status_get(app: &AppHandle) -> Status {
    let state = app.state::<AppState>();

    state.status.lock().map_or_else(|_| status_idle(), |status| status_of(&status))
}

async fn controller_run(
    app: AppHandle,
    mut controller: Controller,
    mut receiver: mpsc::Receiver<Command>,
) {
    // tigerstyle-ignore: TS006 -- the controller serves shortcuts for the life of the app.
    while let Some(command) = receiver.recv().await {
        controller.step(&app, command);
    }

    tracing::error!("the dictation controller stopped because its queue closed");
}

fn cue_play_configured(cue: Cue) {
    if settings_current().cues_enabled {
        audio::cue_play(cue);
    }
}

fn delivery_report(app: &AppHandle, delivered: AppResult<Delivery>) {
    let delivery = match delivered {
        Ok(delivery) => delivery,
        Err(error) => {
            status_update(app, |status| status.jobs = status.jobs.saturating_sub(1));
            error_report(app, &error);

            return;
        }
    };

    if delivery.text.is_none() {
        cue_play_configured(Cue::Error);
    }

    status_update(app, |status| {
        status.jobs = status.jobs.saturating_sub(1);
        status.notice = delivery.notice;

        if delivery.text.is_some() {
            status.error = None;
        }
    });
}

async fn cleanup_run(settings: &Settings, transcript: &str) -> AppResult<String> {
    let endpoint = endpoint_check(settings, KeyPurpose::Llm)?;
    let model = settings.cleanup_model.as_str();
    let thinking = settings.cleanup_thinking;

    service::cleanup(&endpoint, model, thinking, transcript, &settings.vocabulary).await
}

fn endpoint_check(settings: &Settings, purpose: KeyPurpose) -> AppResult<Endpoint> {
    let root = settings::root_resolve()?;

    service::endpoint_resolve(&root, settings, purpose)
}

const fn phase_of(state: &StatusState) -> Phase {
    if state.listening {
        return Phase::Listening;
    }

    if state.jobs > 0 {
        return Phase::Transcribing;
    }

    Phase::Idle
}

const fn press_stops(mode: ShortcutMode) -> bool {
    match mode {
        ShortcutMode::Hold => false,
        ShortcutMode::HoldOrTap | ShortcutMode::Tap => true,
    }
}

fn release_stops(mode: ShortcutMode, held: Duration) -> bool {
    match mode {
        ShortcutMode::Hold => true,
        ShortcutMode::HoldOrTap => held >= Duration::from_millis(HOLD_MS_MIN),
        ShortcutMode::Tap => false,
    }
}

async fn recording_deliver(app: &AppHandle, recording: Recording) -> AppResult<Delivery> {
    let started = Instant::now();
    let settings = settings_current();
    let endpoint = endpoint_check(&settings, KeyPurpose::Transcription)?;
    let audio_ms = recording_ms(&recording);
    let (peak_db, loudest_db) = audio::recording_levels(&recording);

    tracing::info!("audio peak {peak_db:.1} dBFS, loudest speech {loudest_db:.1} dBFS");

    let encoded = tauri::async_runtime::spawn_blocking(move || audio::recording_encode(&recording))
        .await
        .map_err(|error| AppError::Task(format!("the audio could not be prepared: {error}")))??;

    let Some(wav) = encoded else {
        return Ok(Delivery { notice: Some(NOTICE_SILENT.to_owned()), text: None });
    };

    let encoded_ms = started.elapsed().as_millis();
    let transcript = service::transcribe(&endpoint, &wav, &settings.vocabulary).await?;
    let transcribed_ms = started.elapsed().as_millis();

    tracing::info!(
        "transcribed {} words from {audio_ms} ms of audio",
        transcript.split_whitespace().count()
    );

    if transcript.is_empty() {
        return Ok(Delivery { notice: Some(NOTICE_SILENT.to_owned()), text: None });
    }

    let (text, notice) = transcript_finish(&settings, transcript).await;
    let cleaned_ms = started.elapsed().as_millis();

    let focused = app
        .get_webview_window(tray::WINDOW_MAIN)
        .is_some_and(|window| window.is_focused().unwrap_or(false));

    let pasted = keystroke::text_deliver(app, &text, settings.paste_keys, !focused).await?;
    let pasted_ms = started.elapsed().as_millis();

    tracing::info!(
        "delivered in {pasted_ms} ms: encode {encoded_ms}, transcribe {}, cleanup {}, paste {}",
        transcribed_ms - encoded_ms,
        cleaned_ms - transcribed_ms,
        pasted_ms - cleaned_ms
    );

    let copied = if focused { NOTICE_COPIED_FOCUSED } else { NOTICE_COPIED };
    let notice = notice.or_else(|| (!pasted).then(|| copied.to_owned()));

    Ok(Delivery { notice, text: Some(text) })
}

fn recording_ms(recording: &Recording) -> u64 {
    debug_assert!(recording.sample_rate > 0);

    let samples = recording.samples.len() as u64;
    let duration_ms = (samples * 1000).div_euclid(u64::from(recording.sample_rate));

    debug_assert!(duration_ms * u64::from(recording.sample_rate) <= samples * 1000);

    duration_ms
}

fn settings_current() -> Settings {
    let loaded = settings::root_resolve().and_then(|root| settings::settings_load(&root));

    loaded.unwrap_or_else(|error| {
        tracing::warn!("the settings could not be read, using defaults: {error}");

        Settings::default()
    })
}

const fn status_idle() -> Status {
    Status { error: None, notice: None, phase: Phase::Idle }
}

fn status_of(state: &StatusState) -> Status {
    Status { error: state.error.clone(), notice: state.notice.clone(), phase: phase_of(state) }
}

fn status_update<Mutate>(app: &AppHandle, mutate: Mutate)
where
    Mutate: FnOnce(&mut StatusState),
{
    let state = app.state::<AppState>();

    let status = {
        let Ok(mut guard) = state.status.lock() else {
            tracing::error!("the status state was poisoned");

            return;
        };

        mutate(&mut guard);

        status_of(&guard)
    };

    if let Err(error) = app.emit(EVENT_STATUS, &status) {
        tracing::warn!("the status change was not announced: {error}");
    }

    tray::tray_phase_show(app, status.phase);
}

async fn transcript_finish(settings: &Settings, transcript: String) -> (String, Option<String>) {
    if !settings.cleanup_enabled {
        return (transcript, None);
    }

    let cleaned = cleanup_run(settings, &transcript).await;

    match cleaned {
        Ok(cleaned) if service::cleanup_accept(&transcript, &cleaned) => (cleaned, None),
        Ok(cleaned) => {
            tracing::warn!(
                "cleanup returned {} words for {} spoken, so the transcript was kept",
                cleaned.split_whitespace().count(),
                transcript.split_whitespace().count()
            );

            let notice = "The LLM cleanup changed the wording, so the raw transcript was pasted \
                          instead.";

            (transcript, Some(notice.to_owned()))
        }
        Err(error) => {
            tracing::warn!("cleanup failed, pasting the transcript as heard: {error}");

            let detail = error.detail();
            let notice =
                format!("The LLM cleanup failed, so the raw transcript was pasted: {detail}");

            (transcript, Some(notice))
        }
    }
}

async fn worker_run(app: AppHandle, mut receiver: mpsc::Receiver<Recording>) {
    // tigerstyle-ignore: TS006 -- the worker serves recordings for the life of the app.
    while let Some(recording) = receiver.recv().await {
        let delivered = recording_deliver(&app, recording).await;

        delivery_report(&app, delivered);
    }

    tracing::error!("the dictation worker stopped because its queue closed");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording_silent(count: usize, sample_rate: u32) -> Recording {
        Recording { sample_rate, samples: vec![0.0; count] }
    }

    #[test]
    fn listening_outranks_pending_work_and_idle_needs_neither() {
        let mut state = StatusState::default();

        assert_eq!(status_of(&state).phase, Phase::Idle);

        state.jobs = 2;

        assert_eq!(status_of(&state).phase, Phase::Transcribing);

        state.listening = true;

        assert_eq!(status_of(&state).phase, Phase::Listening);

        state.jobs = 0;
        state.listening = false;

        assert_eq!(status_of(&state), status_idle());
    }

    #[test]
    fn each_shortcut_mode_ends_a_recording_the_way_its_name_says() {
        let tap = Duration::from_millis(HOLD_MS_MIN - 1);
        let hold = Duration::from_millis(HOLD_MS_MIN);

        assert!(release_stops(ShortcutMode::Hold, tap));
        assert!(release_stops(ShortcutMode::Hold, hold));
        assert!(!press_stops(ShortcutMode::Hold));

        assert!(!release_stops(ShortcutMode::Tap, tap));
        assert!(!release_stops(ShortcutMode::Tap, hold));
        assert!(press_stops(ShortcutMode::Tap));

        assert!(!release_stops(ShortcutMode::HoldOrTap, tap));
        assert!(release_stops(ShortcutMode::HoldOrTap, hold));
        assert!(press_stops(ShortcutMode::HoldOrTap));
    }

    #[test]
    fn a_recording_lasts_its_samples_over_its_rate_rounded_down() {
        assert_eq!(recording_ms(&recording_silent(0, 16_000)), 0);
        assert_eq!(recording_ms(&recording_silent(16_000, 16_000)), 1000);
        assert_eq!(recording_ms(&recording_silent(15_999, 16_000)), 999);
        assert_eq!(recording_ms(&recording_silent(24_000, 48_000)), 500);
        assert_eq!(recording_ms(&recording_silent(1, 44_100)), 0);
    }

    #[test]
    fn a_status_crosses_the_ipc_boundary_with_snake_case_phases() {
        let status =
            Status { error: None, notice: Some("n".to_owned()), phase: Phase::Transcribing };

        let value = serde_json::to_value(&status).unwrap();

        assert_eq!(value["phase"], "transcribing");
        assert_eq!(value["notice"], "n");
        assert!(value["error"].is_null());
    }
}
