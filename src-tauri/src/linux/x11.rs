use crate::error::{AppError, AppResult};
use crate::keystroke::{Chord, Key};
use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, Window};
use x11rb::protocol::xtest::ConnectionExt as _;

const CHORD_KEYS_MAX: u32 = 4;
const DEVICE_CORE: u8 = 0;

pub(super) fn chord_send(chord: Chord) -> AppResult<()> {
    let (connection, screen_index) = x11rb::connect(None).map_err(x11_error)?;
    let setup = connection.setup();
    let root = setup.roots.get(screen_index).ok_or_else(|| x11_error("no screen"))?.root;
    let keycode_min = setup.min_keycode;
    let keycode_count = setup.max_keycode.saturating_sub(keycode_min).saturating_add(1);

    let mapping = connection
        .get_keyboard_mapping(keycode_min, keycode_count)
        .map_err(x11_error)?
        .reply()
        .map_err(x11_error)?;

    let mut keycodes: Vec<u8> = Vec::with_capacity(CHORD_KEYS_MAX as usize);

    for key in chord.modifiers.iter().chain(std::iter::once(&chord.key)) {
        let keycode =
            keycode_find(&mapping.keysyms, mapping.keysyms_per_keycode, keycode_min, *key)
                .ok_or_else(|| x11_error(format!("the keyboard layout has no {key:?} key")))?;

        keycodes.push(keycode);
    }

    debug_assert_eq!(keycodes.len(), chord.modifiers.len() + 1);

    for keycode in &keycodes {
        key_fake(&connection, root, KEY_PRESS_EVENT, *keycode)?;
    }

    for keycode in keycodes.iter().rev() {
        key_fake(&connection, root, KEY_RELEASE_EVENT, *keycode)?;
    }

    connection.get_input_focus().map_err(x11_error)?.reply().map_err(x11_error)?;

    Ok(())
}

fn key_fake(connection: &impl Connection, root: Window, event: u8, keycode: u8) -> AppResult<()> {
    debug_assert!(event == KEY_PRESS_EVENT || event == KEY_RELEASE_EVENT);

    connection
        .xtest_fake_input(event, keycode, CURRENT_TIME, root, 0, 0, DEVICE_CORE)
        .map_err(x11_error)?;

    Ok(())
}

fn keycode_find(keysyms: &[u32], per_keycode: u8, keycode_min: u8, key: Key) -> Option<u8> {
    let width = usize::from(per_keycode);

    if width == 0 {
        return None;
    }

    let keysym = super::keysym_of(key);
    let rows = keysyms.chunks_exact(width);

    let row = match rows.clone().position(|row| row.first() == Some(&keysym)) {
        Some(row) => row,
        None => rows.clone().position(|row| row.contains(&keysym))?,
    };

    let index = u8::try_from(row).ok()?;

    keycode_min.checked_add(index)
}

fn x11_error(error: impl std::fmt::Display) -> AppError {
    AppError::Keystroke(format!("the X server did not take the paste keys: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keycode_prefers_the_unshifted_column_and_falls_back_to_any_column() {
        let rows = [[0x0061, 0x0041], [0x0056, 0x0076], [0x0076, 0x0056], [0xffe3, 0x0000]];
        let keysyms = rows.concat();

        assert_eq!(keycode_find(&keysyms, 2, 8, Key::V), Some(10));
        assert_eq!(keycode_find(&keysyms, 2, 8, Key::Control), Some(11));
        assert_eq!(keycode_find(&keysyms, 2, 8, Key::Insert), None);
        assert_eq!(keycode_find(&keysyms, 0, 8, Key::V), None);
        assert_eq!(keycode_find(&keysyms[..4], 2, 8, Key::V), Some(9));
    }
}
