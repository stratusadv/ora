use crate::error::{AppError, AppResult};
use crate::keystroke::{Chord, Key};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::{
    self,
    ZwpVirtualKeyboardManagerV1,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::{
    self,
    ZwpVirtualKeyboardV1,
};

const KEY_STATE_PRESSED: u32 = 1;
const KEY_STATE_RELEASED: u32 = 0;
const KEYCODE_INSERT: u32 = 2;
const KEYCODE_V: u32 = 1;
const KEYMAP_FILE_PREFIX: &str = "ora-keymap-";
const KEYMAP_FORMAT_XKB_V1: u32 = 1;
const KEYMAP_MODE: u32 = 0o600;
const MODIFIER_CONTROL: u32 = 1 << 2;
const MODIFIER_SHIFT: u32 = 1;
const TIME_PRESS_MS: u32 = 1;
const TIME_RELEASE_MS: u32 = 2;

const KEYMAP: &str = "xkb_keymap {
    xkb_keycodes \"ora\" {
        minimum = 8;
        maximum = 255;
        <V> = 9;
        <INS> = 10;
    };
    xkb_types \"ora\" { include \"complete\" };
    xkb_compat \"ora\" { include \"complete\" };
    xkb_symbols \"ora\" {
        key <V> { [ v ] };
        key <INS> { [ Insert ] };
    };
};
";

#[derive(Debug, Clone, Copy, Default)]
struct KeyboardState;

impl Dispatch<WlRegistry, GlobalListContents> for KeyboardState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for KeyboardState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for KeyboardState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpVirtualKeyboardManagerV1,
        _event: zwp_virtual_keyboard_manager_v1::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for KeyboardState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpVirtualKeyboardV1,
        _event: zwp_virtual_keyboard_v1::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

pub(super) fn chord_send(chord: Chord) -> AppResult<()> {
    let key = keycode_of(chord.key)?;
    let modifiers =
        chord.modifiers.iter().fold(0, |mask, modifier| mask | modifier_mask(*modifier));
    let connection = Connection::connect_to_env().map_err(wayland_error)?;
    let (globals, mut queue) =
        registry_queue_init::<KeyboardState>(&connection).map_err(wayland_error)?;
    let handle = queue.handle();
    let seat: WlSeat = globals.bind(&handle, 1..=1, ()).map_err(wayland_error)?;
    let manager: ZwpVirtualKeyboardManagerV1 =
        globals.bind(&handle, 1..=1, ()).map_err(wayland_error)?;
    let keyboard = manager.create_virtual_keyboard(&seat, &handle, ());
    let keymap = keymap_file()?;
    let keymap_size = u32::try_from(KEYMAP.len() + 1).map_err(wayland_error)?;

    debug_assert_ne!(modifiers, 0);

    keyboard.keymap(KEYMAP_FORMAT_XKB_V1, keymap.as_fd(), keymap_size);
    keyboard.modifiers(modifiers, 0, 0, 0);
    keyboard.key(TIME_PRESS_MS, key, KEY_STATE_PRESSED);
    keyboard.key(TIME_RELEASE_MS, key, KEY_STATE_RELEASED);
    keyboard.modifiers(0, 0, 0, 0);
    queue.roundtrip(&mut KeyboardState).map_err(wayland_error)?;
    keyboard.destroy();
    queue.roundtrip(&mut KeyboardState).map_err(wayland_error)?;

    Ok(())
}

pub(super) fn virtual_keyboard_available() -> bool {
    let Ok(connection) = Connection::connect_to_env() else {
        return false;
    };

    let Ok((globals, _queue)) = registry_queue_init::<KeyboardState>(&connection) else {
        return false;
    };

    let interface = ZwpVirtualKeyboardManagerV1::interface().name;

    globals.contents().with_list(|list| list.iter().any(|global| global.interface == interface))
}

fn keycode_of(key: Key) -> AppResult<u32> {
    match key {
        Key::Insert => Ok(KEYCODE_INSERT),
        Key::V => Ok(KEYCODE_V),
        Key::Control | Key::Shift => Err(AppError::Keystroke(format!(
            "{key:?} is a modifier and has no key of its own in ora's keymap"
        ))),
    }
}

fn keymap_file() -> AppResult<File> {
    let directory =
        std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = directory.join(format!("{KEYMAP_FILE_PREFIX}{}", std::process::id()));

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .mode(KEYMAP_MODE)
        .open(&path)?;

    fs::remove_file(&path)?;
    file.write_all(KEYMAP.as_bytes())?;
    file.write_all(&[0])?;
    file.flush()?;

    debug_assert!(!path.exists());

    Ok(file)
}

const fn modifier_mask(key: Key) -> u32 {
    match key {
        Key::Control => MODIFIER_CONTROL,
        Key::Shift => MODIFIER_SHIFT,
        Key::Insert | Key::V => 0,
    }
}

fn wayland_error(error: impl std::fmt::Display) -> AppError {
    AppError::Keystroke(format!("the compositor did not accept the virtual keyboard: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystroke::chord_for;
    use crate::settings::PasteKeys;

    #[test]
    fn the_keymap_names_every_key_a_chord_can_press_at_its_evdev_offset() {
        assert!(KEYMAP.contains("<V> = 9;"));
        assert!(KEYMAP.contains("<INS> = 10;"));
        assert!(KEYMAP.contains("key <V> { [ v ] };"));
        assert!(KEYMAP.contains("key <INS> { [ Insert ] };"));
        assert_eq!(keycode_of(Key::V).unwrap() + 8, 9);
        assert_eq!(keycode_of(Key::Insert).unwrap() + 8, 10);
        assert!(keycode_of(Key::Control).is_err());
    }

    #[test]
    fn every_paste_chord_maps_to_real_modifier_bits() {
        for keys in [PasteKeys::ControlV, PasteKeys::ControlShiftV, PasteKeys::ShiftInsert] {
            let chord = chord_for(keys);
            let mask =
                chord.modifiers.iter().fold(0, |mask, modifier| mask | modifier_mask(*modifier));

            assert_ne!(mask, 0);
            assert!(keycode_of(chord.key).is_ok());
        }

        assert_eq!(modifier_mask(Key::Control) | modifier_mask(Key::Shift), 0b101);
    }
}
