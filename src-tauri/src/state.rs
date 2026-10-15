use crate::dictation::{Dictation, StatusState};
use crate::monitor::MonitorState;
use crate::platform::{self, KeystrokeBackend, PlatformState, ShortcutBackend};
use crate::tray::TrayIcons;
use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, Manager};
use tokio::sync::OnceCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct Backends {
    pub(crate) keystroke: KeystrokeBackend,
    pub(crate) shortcut: ShortcutBackend,
}

pub(crate) struct AppState {
    pub(crate) backends: OnceCell<Backends>,
    pub(crate) clipboard: Mutex<Option<arboard::Clipboard>>,
    pub(crate) dictation: OnceLock<Dictation>,
    pub(crate) monitor: Mutex<MonitorState>,
    pub(crate) platform: PlatformState,
    pub(crate) shortcut: Mutex<ShortcutStatus>,
    pub(crate) status: Mutex<StatusState>,
    pub(crate) tray_icons: OnceLock<TrayIcons>,
    pub(crate) window_build_started: Mutex<Option<Instant>>,
}

impl AppState {
    pub(crate) fn new() -> Self {
        Self {
            backends: OnceCell::new(),
            clipboard: Mutex::new(None),
            dictation: OnceLock::new(),
            monitor: Mutex::new(MonitorState::default()),
            platform: PlatformState::new(),
            shortcut: Mutex::new(ShortcutStatus::default()),
            status: Mutex::new(StatusState::default()),
            tray_icons: OnceLock::new(),
            window_build_started: Mutex::new(None),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ShortcutStatus {
    pub(crate) bound: bool,
    pub(crate) error: Option<String>,
}

pub(crate) async fn backends_get(app: &AppHandle) -> Backends {
    let state = app.state::<AppState>();
    let backends = *state.backends.get_or_init(|| platform::backends_detect(&state.platform)).await;

    debug_assert!(state.backends.initialized());

    backends
}
