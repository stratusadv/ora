use crate::audio::{self, Microphone};
use crate::dictation::{self, Status};
use crate::error::{AppError, AppResult};
use crate::monitor;
use crate::platform::{self, KeystrokeBackend, ShortcutBackend};
use crate::service::{self, KeyPurpose, ServiceStatus};
use crate::settings::{self, Settings};
use crate::shortcut;
use crate::state::{self, AppState};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Platform {
    desktop: String,
    keystroke: KeystrokeBackend,
    os: &'static str,
    shortcut: ShortcutBackend,
    shortcut_bound: bool,
    shortcut_error: Option<String>,
    typing_allowed: bool,
}

#[tauri::command]
pub(crate) async fn app_quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub(crate) async fn app_relaunch(app: AppHandle) {
    app.restart();
}

#[tauri::command]
pub(crate) async fn autostart_get(app: AppHandle) -> AppResult<bool> {
    app.autolaunch().is_enabled().map_err(autostart_error)
}

#[tauri::command]
pub(crate) async fn autostart_set(app: AppHandle, enabled: bool) -> AppResult<bool> {
    let manager = app.autolaunch();

    if enabled {
        manager.enable().map_err(autostart_error)?;
    } else {
        manager.disable().map_err(autostart_error)?;
    }

    let current = manager.is_enabled().map_err(autostart_error)?;

    if current != enabled {
        return Err(AppError::Config(
            "the system did not apply the launch-at-startup setting; check the startup applications"
                .into(),
        ));
    }

    Ok(current)
}

#[tauri::command]
pub(crate) async fn microphones_list() -> AppResult<Vec<Microphone>> {
    tauri::async_runtime::spawn_blocking(audio::microphones_list)
        .await
        .map_err(|error| AppError::Task(format!("the microphones could not be listed: {error}")))?
}

#[tauri::command]
pub(crate) async fn models_list(purpose: KeyPurpose) -> AppResult<Vec<service::ModelInfo>> {
    let root = settings::root_resolve()?;
    let settings = settings::settings_load(&root)?;
    let endpoint = service::endpoint_resolve(&root, &settings, purpose)?;

    service::models_list(&endpoint).await
}

#[tauri::command]
pub(crate) async fn monitor_close(app: AppHandle) {
    monitor::monitor_close(&app);
}

#[tauri::command]
pub(crate) async fn monitor_open(app: AppHandle) {
    monitor::monitor_open(&app);
}

#[tauri::command]
pub(crate) async fn platform_get(app: AppHandle) -> AppResult<Platform> {
    platform_describe(&app).await
}

#[tauri::command]
pub(crate) async fn service_get() -> AppResult<ServiceStatus> {
    let root = settings::root_resolve()?;
    let settings = settings::settings_load(&root)?;

    service::service_status(&root, &settings)
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn service_key_set(
    purpose: KeyPurpose,
    api_key: String,
) -> AppResult<ServiceStatus> {
    let root = settings::root_resolve()?;
    let settings = settings::settings_load(&root)?;

    service::api_key_save(&root, purpose, &api_key)?;

    service::service_status(&root, &settings)
}

#[tauri::command]
pub(crate) async fn settings_get() -> AppResult<Settings> {
    let root = settings::root_resolve()?;

    settings::settings_load(&root)
}

#[tauri::command]
pub(crate) async fn settings_update(app: AppHandle, settings: Settings) -> AppResult<Settings> {
    let normalized = settings.normalized()?;
    let root = settings::root_resolve()?;
    let backends = state::backends_get(&app).await;

    shortcut::accelerator_validate(&normalized.shortcut)?;

    if backends.shortcut.hotkey() {
        let previous = settings::settings_load(&root)?;

        shortcut::hotkey_replace(&app, &previous.shortcut, &normalized.shortcut)?;
    }

    if backends.shortcut.keyboard() {
        platform::keyboard_chord_set(&app.state::<AppState>().platform, &normalized.shortcut)?;
    }

    settings::settings_save(&root, &normalized)?;

    Ok(normalized)
}

#[tauri::command]
pub(crate) async fn dictation_toggle(app: AppHandle) {
    dictation::command_send(&app, dictation::Command::Toggled);
}

#[tauri::command]
pub(crate) async fn status_get(app: AppHandle) -> Status {
    dictation::status_get(&app)
}

#[tauri::command]
pub(crate) async fn typing_allow(app: AppHandle) -> AppResult<Platform> {
    let root = settings::root_resolve()?;

    platform::typing_allow(&root).await?;

    platform_describe(&app).await
}

fn autostart_error(error: impl std::fmt::Display) -> AppError {
    AppError::Config(format!("the launch-at-startup setting could not be changed: {error}"))
}

async fn platform_describe(app: &AppHandle) -> AppResult<Platform> {
    let backends = state::backends_get(app).await;
    let shortcut = shortcut::shortcut_status(app);
    let root = settings::root_resolve()?;

    let platform = Platform {
        desktop: platform::desktop_name(),
        keystroke: backends.keystroke,
        os: std::env::consts::OS,
        shortcut: backends.shortcut,
        shortcut_bound: shortcut.bound,
        shortcut_error: shortcut.error,
        typing_allowed: platform::typing_allowed(&root),
    };

    debug_assert_ne!(platform.desktop, "");

    Ok(platform)
}
