use crate::dictation::{self, Command};
use crate::error::{AppError, AppResult};
use crate::platform;
use crate::settings::{self, SHORTCUT_DEFAULT};
use crate::state::{AppState, ShortcutStatus};
use std::str::FromStr;
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tauri_plugin_global_shortcut::{
    Code,
    GlobalShortcut,
    Modifiers,
    Shortcut,
    ShortcutEvent,
    ShortcutState,
};

pub(crate) const EVENT_PLATFORM: &str = "platform";
const FUNCTION_KEY_FIRST: u8 = 1;
const FUNCTION_KEY_LAST: u8 = 24;
const KEYS_SILENT: [Code; 2] = [Code::Pause, Code::ScrollLock];

pub(crate) fn accelerator_validate(accelerator: &str) -> AppResult<()> {
    let parsed = Shortcut::from_str(accelerator).map_err(|error| {
        AppError::Shortcut(format!("{accelerator} is not a shortcut ora understands: {error}"))
    })?;

    if parsed.mods.intersects(Modifiers::ALT | Modifiers::CONTROL | Modifiers::SUPER) {
        return Ok(());
    }

    if key_is_function(parsed.key) {
        return Ok(());
    }

    if KEYS_SILENT.contains(&parsed.key) {
        return Ok(());
    }

    Err(AppError::Shortcut(format!(
        "{accelerator} would take over normal typing; add Ctrl, Alt, or Super, or use a key that \
         never types, such as F8 or Scroll Lock."
    )))
}

pub(crate) fn bound_set(app: &AppHandle) {
    shortcut_status_update(app, |status| {
        status.bound = true;
        status.error = None;
    });
}

pub(crate) fn hotkey_install(app: &AppHandle, accelerator: &str) -> AppResult<()> {
    let plugin = tauri_plugin_global_shortcut::Builder::new().with_handler(hotkey_event_handle);

    app.plugin(plugin.build()).map_err(|error| {
        AppError::Shortcut(format!("the shortcut service did not start: {error}"))
    })?;

    hotkey_register(app, accelerator)
}

pub(crate) fn hotkey_replace(app: &AppHandle, previous: &str, next: &str) -> AppResult<()> {
    let manager = hotkey_manager(app)?;

    if manager.is_registered(next) {
        return Ok(());
    }

    hotkey_register(app, next)?;

    if let Err(error) = manager.unregister(previous) {
        tracing::warn!("the previous shortcut {previous} could not be released: {error}");
    }

    debug_assert!(manager.is_registered(next));

    Ok(())
}

pub(crate) fn hotkey_start(app: &AppHandle) {
    let accelerator = settings::root_resolve()
        .and_then(|root| settings::settings_load(&root))
        .map_or_else(|_| SHORTCUT_DEFAULT.to_owned(), |settings| settings.shortcut);

    let Err(error) = hotkey_install(app, &accelerator) else {
        return;
    };

    let detail = error.detail();

    tracing::warn!("the shortcut {accelerator} did not register: {detail}");

    shortcut_status_update(app, |status| {
        status.bound = false;
        status.error = Some(detail);
    });
}

pub(crate) fn keyboard_start(app: &AppHandle) {
    let accelerator = settings::root_resolve()
        .and_then(|root| settings::settings_load(&root))
        .map_or_else(|_| SHORTCUT_DEFAULT.to_owned(), |settings| settings.shortcut);

    let state = app.state::<AppState>();

    if let Err(error) = platform::keyboard_chord_set(&state.platform, &accelerator) {
        let detail = error.detail();

        tracing::warn!("the shortcut {accelerator} cannot be read from the keyboard: {detail}");

        if let Err(error) = platform::keyboard_chord_set(&state.platform, SHORTCUT_DEFAULT) {
            unbound_set(app, error.detail());

            return;
        }
    }

    platform::keyboard_listen_start(app);
}

pub(crate) fn shortcut_status(app: &AppHandle) -> ShortcutStatus {
    let state = app.state::<AppState>();

    state.shortcut.lock().map(|status| status.clone()).unwrap_or_default()
}

pub(crate) fn unbound_set(app: &AppHandle, error: String) {
    debug_assert_ne!(error, "");

    tracing::warn!("the shortcut is not working: {error}");

    shortcut_status_update(app, |status| {
        status.bound = false;
        status.error = Some(error);
    });
}

fn hotkey_event_handle(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    let command = match event.state() {
        ShortcutState::Pressed => Command::ShortcutPressed,
        ShortcutState::Released => Command::ShortcutReleased,
    };

    dictation::command_send(app, command);
}

fn hotkey_manager(app: &AppHandle) -> AppResult<State<'_, GlobalShortcut<Wry>>> {
    app.try_state::<GlobalShortcut<Wry>>()
        .ok_or_else(|| AppError::Shortcut("the shortcut service is not running".into()))
}

fn hotkey_register(app: &AppHandle, accelerator: &str) -> AppResult<()> {
    let manager = hotkey_manager(app)?;

    accelerator_validate(accelerator)?;

    manager.register(accelerator).map_err(|error| {
        AppError::Shortcut(format!(
            "{accelerator} could not be registered; another app may already use it ({error})"
        ))
    })?;

    debug_assert!(manager.is_registered(accelerator));

    bound_set(app);

    Ok(())
}

fn key_is_function(key: Code) -> bool {
    let name = key.to_string();

    name.strip_prefix('F')
        .and_then(|digits| digits.parse::<u8>().ok())
        .is_some_and(|number| (FUNCTION_KEY_FIRST..=FUNCTION_KEY_LAST).contains(&number))
}

fn shortcut_status_update<Mutate>(app: &AppHandle, mutate: Mutate)
where
    Mutate: FnOnce(&mut ShortcutStatus),
{
    {
        let state = app.state::<AppState>();

        let Ok(mut status) = state.shortcut.lock() else {
            tracing::error!("the shortcut state was poisoned");

            return;
        };

        mutate(&mut status);
    }

    if let Err(error) = app.emit(EVENT_PLATFORM, ()) {
        tracing::warn!("the platform change was not announced: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accelerators_from_the_settings_page_parse_and_malformed_ones_do_not() {
        assert!(accelerator_validate("Control+Shift+Space").is_ok());
        assert!(accelerator_validate("Control+Alt+KeyD").is_ok());
        assert!(accelerator_validate("Super+F9").is_ok());
        assert!(accelerator_validate("Control+").is_err());
        assert!(accelerator_validate("Control+KeyA+KeyB").is_err());
        assert!(accelerator_validate("Banana").is_err());
    }

    #[test]
    fn a_shortcut_that_would_steal_ordinary_typing_is_refused() {
        assert!(accelerator_validate("KeyA").is_err());
        assert!(accelerator_validate("Shift+KeyA").is_err());
        assert!(accelerator_validate("Space").is_err());
        assert!(accelerator_validate("Shift+Digit1").is_err());
        assert!(accelerator_validate("F8").is_ok());
        assert!(accelerator_validate("Shift+F24").is_ok());
        assert!(accelerator_validate("Alt+Space").is_ok());
        assert!(accelerator_validate("ScrollLock").is_ok());
        assert!(accelerator_validate("Pause").is_ok());
        assert!(accelerator_validate("CapsLock").is_err());
        assert!(accelerator_validate("Insert").is_err());
        assert!(accelerator_validate("PrintScreen").is_err());
    }

    #[test]
    fn function_keys_are_recognized_by_name_from_one_to_twenty_four() {
        assert!(key_is_function(Code::F1));
        assert!(key_is_function(Code::F24));
        assert!(!key_is_function(Code::KeyF));
        assert!(!key_is_function(Code::Space));
    }
}
