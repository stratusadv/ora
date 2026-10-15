mod audio;
mod commands;
mod dictation;
mod error;
mod keystroke;
mod monitor;
#[cfg(target_os = "linux")] // tigerstyle-ignore: TS035
#[path = "linux/mod.rs"]
mod platform;
#[cfg(windows)] // tigerstyle-ignore: TS035
#[path = "windows.rs"]
mod platform;
mod service;
mod settings;
mod shortcut;
mod state;
mod tray;

use crate::state::AppState;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::webview::{PageLoadEvent, PageLoadPayload};
use tauri::{AppHandle, Manager, Webview, Window, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const ARGUMENT_AUTOSTART: &str = "--autostart";
const LOG_FILE_PREFIX: &str = "ora";
const LOG_FILTER_DEFAULT: &str =
    "ora_lib=info,warn,arboard=error,pulseaudio::client::reactor=off,zbus::proxy=error";
const LOGS_FOLDER: &str = "logs";
const UPDATE_CHECK_TIMEOUT_SECONDS: u64 = 15;

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

pub fn run() {
    tracing_initialize();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _arguments, _directory| {
            tray::window_main_show(app);
        }))
        .plugin(tauri_plugin_autostart::Builder::new().arg(ARGUMENT_AUTOSTART).build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::new())
        .setup(|app| {
            setup(app.handle());

            Ok(())
        })
        .on_page_load(page_load_handle)
        .on_window_event(window_event_handle)
        .invoke_handler(tauri::generate_handler![
            commands::app_quit,
            commands::app_relaunch,
            commands::autostart_get,
            commands::autostart_set,
            commands::dictation_toggle,
            commands::microphones_list,
            commands::models_list,
            commands::monitor_close,
            commands::monitor_open,
            commands::platform_get,
            commands::service_get,
            commands::service_key_set,
            commands::settings_get,
            commands::settings_update,
            commands::status_get,
            commands::typing_allow,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        tracing::error!("ora exited with an error: {error}");
    }
}

fn autostart_launched() -> bool {
    std::env::args().any(|argument| argument == ARGUMENT_AUTOSTART)
}

fn page_load_handle(webview: &Webview, payload: &PageLoadPayload<'_>) {
    if payload.event() != PageLoadEvent::Finished {
        return;
    }

    if webview.label() != tray::WINDOW_MAIN {
        return;
    }

    if let Some(window) = webview.app_handle().get_webview_window(tray::WINDOW_MAIN) {
        tray::window_reveal(&window);
    }
}

fn setup(app: &AppHandle) {
    if let Err(error) = tray::tray_create(app) {
        tracing::warn!("ora is running without a tray icon: {error}");
    }

    let dictation = dictation::dictation_start(app);
    let stored = app.state::<AppState>().dictation.set(dictation).is_ok();

    debug_assert!(stored);

    if !platform::session_wayland() {
        shortcut::hotkey_start(app);
    }

    tauri::async_runtime::spawn(startup_run(app.clone()));

    if autostart_launched() {
        tauri::async_runtime::spawn(update_check_run(app.clone()));
    } else {
        tray::window_main_show(app);
    }
}

async fn startup_run(app: AppHandle) {
    let backends = state::backends_get(&app).await;

    tracing::info!(
        "input backends: shortcut {:?}, keystroke {:?}",
        backends.shortcut,
        backends.keystroke
    );

    if backends.shortcut.hotkey() {
        debug_assert!(!platform::session_wayland());

        return;
    }

    debug_assert!(backends.shortcut.keyboard());

    shortcut::keyboard_start(&app);
}

fn tracing_directory() -> PathBuf {
    let resolved = settings::root_resolve().map(|root| root.join(LOGS_FOLDER));

    if let Ok(directory) = resolved {
        if std::fs::create_dir_all(&directory).is_ok() {
            return directory;
        }
    }

    let fallback = std::env::temp_dir().join(settings::APP_DATA_FOLDER).join(LOGS_FOLDER);

    debug_assert!(fallback.is_absolute());

    fallback
}

fn tracing_initialize() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(LOG_FILTER_DEFAULT));

    let directory = tracing_directory();
    let appender = tracing_appender::rolling::daily(&directory, LOG_FILE_PREFIX);
    let (writer, guard) = tracing_appender::non_blocking(appender);

    drop(LOG_GUARD.set(guard));

    let layer_stdout = tracing_subscriber::fmt::layer().with_target(false).with_ansi(true);

    let layer_file =
        tracing_subscriber::fmt::layer().with_target(true).with_ansi(false).with_writer(writer);

    let installed =
        tracing_subscriber::registry().with(filter).with(layer_stdout).with(layer_file).try_init();

    if installed.is_err() {
        return;
    }

    debug_assert!(LOG_GUARD.get().is_some());

    tracing::info!("logging to {}", directory.display());
}

async fn update_check_run(app: AppHandle) {
    let updater =
        app.updater_builder().timeout(Duration::from_secs(UPDATE_CHECK_TIMEOUT_SECONDS)).build();

    let checked = match updater {
        Ok(updater) => updater.check().await,
        Err(error) => {
            tracing::warn!("the updater did not start: {error}");

            return;
        }
    };

    let update = match checked {
        Ok(Some(update)) => update,
        Ok(None) => return,
        Err(error) => {
            tracing::info!("the update check failed: {error}");

            return;
        }
    };

    let shown = app
        .notification()
        .builder()
        .title("An ora update is available")
        .body(format!("Open ora to install version {}.", update.version))
        .show();

    if let Err(error) = shown {
        tracing::warn!("the update notification was not shown: {error}");
    }
}

fn window_event_handle(window: &Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };

    if window.label() != tray::WINDOW_MAIN {
        return;
    }

    api.prevent_close();

    if let Err(error) = window.hide() {
        tracing::warn!("the settings window did not hide: {error}");
    }

    monitor::monitor_pause(window.app_handle());
}
