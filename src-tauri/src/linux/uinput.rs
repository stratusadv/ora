use crate::error::{AppError, AppResult};
use crate::keystroke::{Chord, Key};
use std::ffi::{c_int, c_ulong};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::Mutex;

const DEVICE_NAME: &[u8] = b"ora paste keyboard";
const DEVICE_PATH: &str = "/dev/uinput";
const EVENT_BYTES: usize = 24;
const EVENT_KEY: u16 = 1;
const EVENT_SYNCHRONIZE: u16 = 0;
const INPUT_BUS_VIRTUAL: u16 = 0x06;
const KEY_INSERT: u16 = 110;
const KEY_LEFTCTRL: u16 = 29;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_V: u16 = 47;
const KEYS: [u16; 4] = [KEY_INSERT, KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_V];
const NAME_BYTES: usize = 80;
const SETUP_BYTES: usize = 92;
const SYNCHRONIZE_REPORT: u16 = 0;
const UI_DEV_CREATE: c_ulong = 0x5501;
const UI_DEV_SETUP: c_ulong = 0x405c_5503;
const UI_SET_EVBIT: c_ulong = 0x4004_5564;
const UI_SET_KEYBIT: c_ulong = 0x4004_5565;
const VALUE_PRESS: i32 = 1;
const VALUE_RELEASE: i32 = 0;

const _: () = assert!(DEVICE_NAME.len() < NAME_BYTES);
const _: () = assert!(SETUP_BYTES == 8 + NAME_BYTES + 4);
const _: () = assert!(UI_DEV_SETUP >> 16 & 0x3fff == SETUP_BYTES as c_ulong);

unsafe extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}

pub(super) fn chord_send(slot: &Mutex<Option<File>>, chord: Chord) -> AppResult<()> {
    debug_assert!(!chord.key.is_modifier());

    let events = events_build(chord);
    let mut guard = slot.lock().map_err(|error| uinput_error(format!("poisoned: {error}")))?;
    let device = guard.as_mut().ok_or_else(|| uinput_error("the virtual keyboard is not open"))?;
    let written = device.write_all(&events).map_err(|error| uinput_error(error.to_string()));

    drop(guard);

    written
}

pub(super) fn open(slot: &Mutex<Option<File>>) -> bool {
    let Ok(mut guard) = slot.lock() else {
        return false;
    };

    if guard.is_some() {
        return true;
    }

    match device_create() {
        Ok(file) => {
            *guard = Some(file);

            true
        }
        Err(error) => {
            tracing::info!("the virtual keyboard is unavailable: {error}");

            false
        }
    }
}

fn device_create() -> AppResult<File> {
    let file = OpenOptions::new().write(true).open(DEVICE_PATH)?;
    let fd = file.as_raw_fd();

    ioctl_int(fd, UI_SET_EVBIT, c_int::from(EVENT_KEY))?;

    for key in KEYS {
        ioctl_int(fd, UI_SET_KEYBIT, c_int::from(key))?;
    }

    let setup = setup_bytes();

    // SAFETY: `fd` is open for the life of `file`, and UI_DEV_SETUP reads exactly SETUP_BYTES
    // from the pointer, which is the length of `setup`.
    let configured = unsafe { ioctl(fd, UI_DEV_SETUP, setup.as_ptr()) };

    if configured < 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    // SAFETY: `fd` is open for the life of `file`, and UI_DEV_CREATE takes no argument.
    let created = unsafe { ioctl(fd, UI_DEV_CREATE) };

    if created < 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(file)
}

fn event_of(kind: u16, code: u16, value: i32) -> [u8; EVENT_BYTES] {
    let mut event = [0_u8; EVENT_BYTES];

    let fields =
        kind.to_ne_bytes().into_iter().chain(code.to_ne_bytes()).chain(value.to_ne_bytes());

    for (slot, byte) in event.iter_mut().skip(16).zip(fields) {
        *slot = byte;
    }

    debug_assert!(event.starts_with(&[0; 16]));

    event
}

fn events_build(chord: Chord) -> Vec<u8> {
    let mut events: Vec<u8> = Vec::with_capacity(EVENT_BYTES * (chord.modifiers.len() + 1) * 4);

    for key in chord.modifiers.iter().chain(std::iter::once(&chord.key)) {
        events.extend_from_slice(&event_of(EVENT_KEY, key_code_of(*key), VALUE_PRESS));
        events.extend_from_slice(&event_of(EVENT_SYNCHRONIZE, SYNCHRONIZE_REPORT, 0));
    }

    for key in std::iter::once(&chord.key).chain(chord.modifiers.iter().rev()) {
        events.extend_from_slice(&event_of(EVENT_KEY, key_code_of(*key), VALUE_RELEASE));
        events.extend_from_slice(&event_of(EVENT_SYNCHRONIZE, SYNCHRONIZE_REPORT, 0));
    }

    debug_assert_eq!(events.len(), EVENT_BYTES * (chord.modifiers.len() + 1) * 4);

    events
}

fn ioctl_int(fd: c_int, request: c_ulong, value: c_int) -> AppResult<()> {
    // SAFETY: `fd` is a valid open descriptor, and these requests take an int by value.
    let result = unsafe { ioctl(fd, request, value) };

    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(())
}

const fn key_code_of(key: Key) -> u16 {
    match key {
        Key::Control => KEY_LEFTCTRL,
        Key::Insert => KEY_INSERT,
        Key::Shift => KEY_LEFTSHIFT,
        Key::V => KEY_V,
    }
}

fn setup_bytes() -> [u8; SETUP_BYTES] {
    let mut setup = [0_u8; SETUP_BYTES];
    let bus = INPUT_BUS_VIRTUAL.to_ne_bytes();

    for (slot, byte) in setup.iter_mut().zip(bus) {
        *slot = byte;
    }

    for (slot, byte) in setup.iter_mut().skip(8).zip(DEVICE_NAME) {
        *slot = *byte;
    }

    debug_assert_eq!(setup.get(8 + DEVICE_NAME.len()), Some(&0));

    setup
}

fn uinput_error(detail: impl std::fmt::Display) -> AppError {
    AppError::Keystroke(format!("the virtual keyboard did not type: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(events: &[u8]) -> Vec<(u16, u16, i32)> {
        events
            .as_chunks::<EVENT_BYTES>()
            .0
            .iter()
            .map(|event| {
                let kind = u16::from_ne_bytes([event[16], event[17]]);
                let code = u16::from_ne_bytes([event[18], event[19]]);
                let value = i32::from_ne_bytes([event[20], event[21], event[22], event[23]]);

                (kind, code, value)
            })
            .collect()
    }

    fn request_encoded(number: c_ulong, size: c_ulong) -> c_ulong {
        (1 << 30) | (size << 16) | (0x55 << 8) | number
    }

    #[test]
    fn the_ioctl_numbers_match_the_kernel_encoding() {
        assert_eq!(UI_SET_EVBIT, request_encoded(100, 4));
        assert_eq!(UI_SET_KEYBIT, request_encoded(101, 4));
        assert_eq!(UI_DEV_SETUP, request_encoded(3, SETUP_BYTES as c_ulong));
        assert_eq!(UI_DEV_CREATE, (0x55 << 8) | 1);
    }

    #[test]
    fn a_chord_presses_in_order_releases_in_reverse_and_syncs_after_each_key() {
        let chord = Chord { key: Key::V, modifiers: &[Key::Control, Key::Shift] };
        let events = events_build(chord);

        let keys: Vec<(u16, i32)> = decoded(&events)
            .into_iter()
            .filter(|(kind, _, _)| *kind == EVENT_KEY)
            .map(|(_, code, value)| (code, value))
            .collect();

        assert_eq!(
            keys,
            [
                (KEY_LEFTCTRL, 1),
                (KEY_LEFTSHIFT, 1),
                (KEY_V, 1),
                (KEY_V, 0),
                (KEY_LEFTSHIFT, 0),
                (KEY_LEFTCTRL, 0),
            ]
        );
        assert_eq!(
            decoded(&events).iter().filter(|(kind, _, _)| *kind == EVENT_SYNCHRONIZE).count(),
            6
        );
    }

    #[test]
    fn the_device_setup_names_a_virtual_bus_device_with_a_terminated_name() {
        let setup = setup_bytes();

        assert_eq!(u16::from_ne_bytes([setup[0], setup[1]]), INPUT_BUS_VIRTUAL);
        assert_eq!(&setup[8..8 + DEVICE_NAME.len()], DEVICE_NAME);
        assert!(setup[8 + DEVICE_NAME.len()..].iter().all(|byte| *byte == 0));
    }
}
