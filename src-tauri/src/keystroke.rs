use crate::error::{AppError, AppResult};
use crate::platform;
use crate::settings::{self, PasteKeys};
use crate::state::{self, AppState};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const CLIPBOARD_SETTLE_MS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Control,
    Insert,
    Shift,
    V,
}

impl Key {
    pub(crate) const fn is_modifier(self) -> bool {
        matches!(self, Self::Control | Self::Shift)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Chord {
    pub(crate) key: Key,
    pub(crate) modifiers: &'static [Key],
}

pub(crate) const fn chord_for(keys: PasteKeys) -> Chord {
    match keys {
        PasteKeys::ControlV => Chord { key: Key::V, modifiers: &[Key::Control] },
        PasteKeys::ControlShiftV => Chord { key: Key::V, modifiers: &[Key::Control, Key::Shift] },
        PasteKeys::ShiftInsert => Chord { key: Key::Insert, modifiers: &[Key::Shift] },
    }
}

pub(crate) async fn text_deliver(
    app: &AppHandle,
    text: &str,
    keys: PasteKeys,
    paste: bool,
) -> AppResult<bool> {
    debug_assert_ne!(text, "");

    let backends = state::backends_get(app).await;

    clipboard_write(app, text)?;

    if !paste {
        return Ok(false);
    }

    if !backends.keystroke.typing() {
        return Ok(false);
    }

    let chord = chord_for(keys);
    let root = settings::root_resolve()?;
    let state = app.state::<AppState>();

    debug_assert!(!chord.key.is_modifier());

    tokio::time::sleep(Duration::from_millis(CLIPBOARD_SETTLE_MS)).await;
    platform::chord_send(&state.platform, backends.keystroke, chord, &root).await?;

    Ok(true)
}

fn clipboard_write(app: &AppHandle, text: &str) -> AppResult<()> {
    let state = app.state::<AppState>();

    let mut guard = state.clipboard.lock().map_err(|error| {
        AppError::Clipboard(format!("the clipboard state was poisoned: {error}"))
    })?;

    let clipboard = match guard.as_mut() {
        Some(clipboard) => clipboard,
        None => guard.insert(arboard::Clipboard::new().map_err(|error| {
            AppError::Clipboard(format!("the clipboard could not be opened: {error}"))
        })?),
    };

    let copied = clipboard
        .set_text(text)
        .map_err(|error| AppError::Clipboard(format!("the text could not be copied: {error}")));

    drop(guard);

    copied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_paste_choice_presses_one_ordinary_key_under_its_modifiers() {
        for keys in [PasteKeys::ControlV, PasteKeys::ControlShiftV, PasteKeys::ShiftInsert] {
            let chord = chord_for(keys);

            assert!(!chord.key.is_modifier());
            assert_ne!(chord.modifiers.len(), 0);
            assert!(chord.modifiers.iter().all(|modifier| modifier.is_modifier()));
        }

        assert_eq!(chord_for(PasteKeys::ControlV).modifiers, &[Key::Control]);
        assert_eq!(chord_for(PasteKeys::ShiftInsert).key, Key::Insert);
    }
}
