use crate::dictation::{self, Command};
use crate::error::{AppError, AppResult};
use crate::shortcut;
use crate::state::AppState;
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

const CAPABILITIES_BYTES_MAX: u64 = 4096;
const DEVICE_COUNT_MAX: u32 = 64;
const DEVICE_RESCAN_MS: u64 = 3000;
const EVENT_BYTES: usize = 24;
const EVENT_KEY: u16 = 1;
const KEY_A: u16 = 30;
const KEY_CODE_MAX: u16 = 0x2ff;
const KEY_SPACE: u16 = 57;
const KEY_VALUE_PRESS: i32 = 1;
const KEY_VALUE_RELEASE: i32 = 0;
const KEY_WORDS: usize = 12;
const KEY_Z: u16 = 44;
const KEYS_ALT: [u16; 2] = [56, 100];
const KEYS_CONTROL: [u16; 2] = [29, 97];
const KEYS_SHIFT: [u16; 2] = [42, 54];
const KEYS_SUPER: [u16; 2] = [125, 126];
const MESSAGE_DENIED: &str = "ora cannot read the keyboard yet. Run sudo usermod -aG input \
    $USER once, then sign out and back in.";
const MESSAGE_MISSING: &str = "No keyboard is connected.";

const _: () = assert!((KEY_CODE_MAX as usize) < KEY_WORDS * 64);
const _: () = assert!(DEVICE_COUNT_MAX as usize <= 64);

#[derive(Debug)]
pub(super) struct KeyboardState {
    devices_open: Mutex<u64>,
    listening: AtomicBool,
    matcher: Mutex<Matcher>,
}

impl KeyboardState {
    pub(super) const fn new() -> Self {
        Self {
            devices_open: Mutex::new(0),
            listening: AtomicBool::new(false),
            matcher: Mutex::new(Matcher::new()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Chord {
    key: u16,
    modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transition {
    Pressed,
    Released,
}

#[derive(Debug)]
struct Matcher {
    active: bool,
    chord: Option<Chord>,
    pressed: [u64; KEY_WORDS],
}

impl Matcher {
    const fn new() -> Self {
        Self { active: false, chord: None, pressed: [0; KEY_WORDS] }
    }

    const fn chord_set(&mut self, chord: Chord) {
        self.active = false;
        self.chord = Some(chord);
    }

    fn held(&self, code: u16) -> bool {
        key_bit(&self.pressed, code)
    }

    fn key_set(&mut self, code: u16, down: bool) -> Option<Transition> {
        debug_assert!(code <= KEY_CODE_MAX);

        let word = usize::from(code.div_euclid(64));

        if let Some(bits) = self.pressed.get_mut(word) {
            let mask = 1_u64 << (code % 64);

            if down { *bits |= mask } else { *bits &= !mask }
        }

        let chord = self.chord?;

        if down { self.key_set_down(code, chord) } else { self.key_set_up(code, chord) }
    }

    fn key_set_down(&mut self, code: u16, chord: Chord) -> Option<Transition> {
        if self.active {
            return None;
        }

        if code != chord.key {
            return None;
        }

        if self.modifiers_held() != chord.modifiers {
            return None;
        }

        self.active = true;

        Some(Transition::Pressed)
    }

    fn key_set_up(&mut self, code: u16, chord: Chord) -> Option<Transition> {
        if !self.active {
            return None;
        }

        let broken = code == chord.key || !self.modifiers_held().contains(chord.modifiers);

        if !broken {
            return None;
        }

        self.active = false;

        Some(Transition::Released)
    }

    fn modifiers_held(&self) -> Modifiers {
        let mut modifiers = Modifiers::empty();

        for (keys, modifier) in [
            (KEYS_ALT, Modifiers::ALT),
            (KEYS_CONTROL, Modifiers::CONTROL),
            (KEYS_SHIFT, Modifiers::SHIFT),
            (KEYS_SUPER, Modifiers::SUPER),
        ] {
            if keys.iter().any(|key| self.held(*key)) {
                modifiers |= modifier;
            }
        }

        modifiers
    }
}

pub(super) fn chord_set(keyboard: &KeyboardState, accelerator: &str) -> AppResult<()> {
    let chord = chord_parse(accelerator)?;

    let Ok(mut matcher) = keyboard.matcher.lock() else {
        return Err(AppError::Shortcut("the keyboard state was poisoned".into()));
    };

    matcher.chord_set(chord);

    debug_assert!(!matcher.active);

    Ok(())
}

pub(super) fn listen_start(app: &AppHandle) {
    let keyboard = keyboard_state(app);

    if keyboard.listening.swap(true, Ordering::SeqCst) {
        return;
    }

    let watcher = app.clone();

    let spawned = thread::Builder::new().name("ora-keyboard-scan".into()).spawn(move || {
        devices_watch(&watcher);
    });

    if let Err(error) = spawned {
        keyboard.listening.store(false, Ordering::SeqCst);
        shortcut::unbound_set(app, format!("the keyboard watcher did not start: {error}"));
    }
}

fn capabilities_keyboard(text: &str) -> bool {
    let words: Vec<u64> = text
        .split_whitespace()
        .rev()
        .take(KEY_WORDS)
        .filter_map(|word| u64::from_str_radix(word, 16).ok())
        .collect();

    let mut bits = [0_u64; KEY_WORDS];

    for (index, word) in words.iter().enumerate() {
        if let Some(slot) = bits.get_mut(index) {
            *slot = *word;
        }
    }

    [KEY_A, KEY_Z, KEY_SPACE, KEYS_CONTROL[0]].iter().all(|code| key_bit(&bits, *code))
}

fn chord_parse(accelerator: &str) -> AppResult<Chord> {
    let parsed = Shortcut::from_str(accelerator).map_err(|error| {
        AppError::Shortcut(format!("{accelerator} is not a shortcut ora understands: {error}"))
    })?;

    let key = key_code_of(parsed.key).ok_or_else(|| {
        AppError::Shortcut(format!("ora cannot listen for the {} key", parsed.key))
    })?;

    let modifiers =
        parsed.mods & (Modifiers::ALT | Modifiers::CONTROL | Modifiers::SHIFT | Modifiers::SUPER);

    debug_assert!(key <= KEY_CODE_MAX);

    Ok(Chord { key, modifiers })
}

fn device_is_keyboard(index: u32) -> bool {
    let path = format!("/sys/class/input/event{index}/device/capabilities/key");

    let Ok(metadata) = fs::metadata(&path) else {
        return false;
    };

    if metadata.len() > CAPABILITIES_BYTES_MAX {
        return false;
    }

    fs::read_to_string(&path).is_ok_and(|text| capabilities_keyboard(&text))
}

fn device_read(app: &AppHandle, mut file: File) {
    let mut event = [0_u8; EVENT_BYTES];

    // tigerstyle-ignore: TS006 -- a keyboard is read until it is unplugged.
    while file.read_exact(&mut event).is_ok() {
        let Some((code, value)) = event_key_of(&event) else {
            continue;
        };

        key_event(app, code, value);
    }
}

fn device_reader_spawn(app: &AppHandle, index: u32, file: File) -> bool {
    let app = app.clone();

    let spawned = thread::Builder::new().name(format!("ora-keyboard-{index}")).spawn(move || {
        device_read(&app, file);
        devices_open_mark(&app, index, false);
    });

    if let Err(error) = spawned {
        tracing::warn!("the reader for keyboard {index} did not start: {error}");

        return false;
    }

    true
}

fn devices_open_mark(app: &AppHandle, index: u32, open: bool) {
    debug_assert!(index < DEVICE_COUNT_MAX);

    if let Ok(mut devices) = keyboard_state(app).devices_open.lock() {
        if open { *devices |= 1 << index } else { *devices &= !(1 << index) }
    }
}

fn devices_scan(app: &AppHandle) -> Result<(), &'static str> {
    let keyboard = keyboard_state(app);
    let mut denied = false;
    let open = keyboard.devices_open.lock().map_or(0, |devices| *devices);

    for index in 0..DEVICE_COUNT_MAX {
        if open & (1 << index) != 0 {
            continue;
        }

        if !device_is_keyboard(index) {
            continue;
        }

        match File::open(format!("/dev/input/event{index}")) {
            Ok(file) => {
                devices_open_mark(app, index, true);

                if !device_reader_spawn(app, index, file) {
                    devices_open_mark(app, index, false);
                }
            }
            Err(error) if error.kind() == ErrorKind::PermissionDenied => denied = true,
            Err(error) => tracing::debug!("keyboard {index} could not be opened: {error}"),
        }
    }

    let readers = keyboard.devices_open.lock().map_or(0, |devices| devices.count_ones());

    if readers > 0 {
        return Ok(());
    }

    if denied { Err(MESSAGE_DENIED) } else { Err(MESSAGE_MISSING) }
}

fn devices_watch(app: &AppHandle) -> ! {
    let mut reported: Option<Result<(), &str>> = None;

    // tigerstyle-ignore: TS006 -- keyboards are watched for the life of the app.
    loop {
        let outcome = devices_scan(app);

        if reported != Some(outcome) {
            match outcome {
                Ok(()) => shortcut::bound_set(app),
                Err(message) => shortcut::unbound_set(app, message.to_owned()),
            }

            reported = Some(outcome);
        }

        thread::sleep(Duration::from_millis(DEVICE_RESCAN_MS));
    }
}

const fn event_key_of(event: &[u8; EVENT_BYTES]) -> Option<(u16, i32)> {
    let kind = u16::from_ne_bytes([event[16], event[17]]);
    let code = u16::from_ne_bytes([event[18], event[19]]);
    let value = i32::from_ne_bytes([event[20], event[21], event[22], event[23]]);

    if kind != EVENT_KEY {
        return None;
    }

    if code > KEY_CODE_MAX {
        return None;
    }

    match value {
        KEY_VALUE_PRESS | KEY_VALUE_RELEASE => Some((code, value)),
        _ => None,
    }
}

fn key_bit(bits: &[u64; KEY_WORDS], code: u16) -> bool {
    bits.get(usize::from(code.div_euclid(64))).is_some_and(|word| word & (1 << (code % 64)) != 0)
}

const fn key_code_control(code: Code) -> Option<u16> {
    let key = match code {
        Code::CapsLock => 58,
        Code::F1 => 59,
        Code::F2 => 60,
        Code::F3 => 61,
        Code::F4 => 62,
        Code::F5 => 63,
        Code::F6 => 64,
        Code::F7 => 65,
        Code::F8 => 66,
        Code::F9 => 67,
        Code::F10 => 68,
        Code::ScrollLock => 70,
        Code::F11 => 87,
        Code::F12 => 88,
        Code::PrintScreen => 99,
        Code::Home => 102,
        Code::ArrowUp => 103,
        Code::PageUp => 104,
        Code::ArrowLeft => 105,
        Code::ArrowRight => 106,
        Code::End => 107,
        Code::ArrowDown => 108,
        Code::PageDown => 109,
        Code::Insert => 110,
        Code::Delete => 111,
        Code::Pause => 119,
        Code::NumpadMultiply => 55,
        Code::Numpad7 => 71,
        Code::Numpad8 => 72,
        Code::Numpad9 => 73,
        Code::NumpadSubtract => 74,
        Code::Numpad4 => 75,
        Code::Numpad5 => 76,
        Code::Numpad6 => 77,
        Code::NumpadAdd => 78,
        Code::Numpad1 => 79,
        Code::Numpad2 => 80,
        Code::Numpad3 => 81,
        Code::Numpad0 => 82,
        Code::NumpadDecimal => 83,
        Code::IntlBackslash => 86,
        Code::NumpadEnter => 96,
        Code::NumpadDivide => 98,
        Code::ContextMenu => 127,
        Code::F13 => 183,
        Code::F14 => 184,
        Code::F15 => 185,
        Code::F16 => 186,
        Code::F17 => 187,
        Code::F18 => 188,
        Code::F19 => 189,
        Code::F20 => 190,
        Code::F21 => 191,
        Code::F22 => 192,
        Code::F23 => 193,
        Code::F24 => 194,
        _ => return None,
    };

    Some(key)
}

fn key_code_of(code: Code) -> Option<u16> {
    let key = key_code_typing(code).or_else(|| key_code_control(code));

    if let Some(key) = key {
        debug_assert!(key <= KEY_CODE_MAX);
    }

    key
}

const fn key_code_typing(code: Code) -> Option<u16> {
    let key = match code {
        Code::Escape => 1,
        Code::Digit1 => 2,
        Code::Digit2 => 3,
        Code::Digit3 => 4,
        Code::Digit4 => 5,
        Code::Digit5 => 6,
        Code::Digit6 => 7,
        Code::Digit7 => 8,
        Code::Digit8 => 9,
        Code::Digit9 => 10,
        Code::Digit0 => 11,
        Code::Minus => 12,
        Code::Equal => 13,
        Code::Backspace => 14,
        Code::Tab => 15,
        Code::KeyQ => 16,
        Code::KeyW => 17,
        Code::KeyE => 18,
        Code::KeyR => 19,
        Code::KeyT => 20,
        Code::KeyY => 21,
        Code::KeyU => 22,
        Code::KeyI => 23,
        Code::KeyO => 24,
        Code::KeyP => 25,
        Code::BracketLeft => 26,
        Code::BracketRight => 27,
        Code::Enter => 28,
        Code::KeyA => 30,
        Code::KeyS => 31,
        Code::KeyD => 32,
        Code::KeyF => 33,
        Code::KeyG => 34,
        Code::KeyH => 35,
        Code::KeyJ => 36,
        Code::KeyK => 37,
        Code::KeyL => 38,
        Code::Semicolon => 39,
        Code::Quote => 40,
        Code::Backquote => 41,
        Code::Backslash => 43,
        Code::KeyZ => 44,
        Code::KeyX => 45,
        Code::KeyC => 46,
        Code::KeyV => 47,
        Code::KeyB => 48,
        Code::KeyN => 49,
        Code::KeyM => 50,
        Code::Comma => 51,
        Code::Period => 52,
        Code::Slash => 53,
        Code::Space => 57,
        _ => return None,
    };

    Some(key)
}

fn key_event(app: &AppHandle, code: u16, value: i32) {
    let down = value == KEY_VALUE_PRESS;

    let transition = {
        let Ok(mut matcher) = keyboard_state(app).matcher.lock() else {
            tracing::error!("the keyboard state was poisoned");

            return;
        };

        matcher.key_set(code, down)
    };

    match transition {
        Some(Transition::Pressed) => dictation::command_send(app, Command::ShortcutPressed),
        Some(Transition::Released) => dictation::command_send(app, Command::ShortcutReleased),
        None => {}
    }
}

fn keyboard_state(app: &AppHandle) -> &KeyboardState {
    &app.state::<AppState>().inner().platform.keyboard
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITIES_KEYBOARD: &str =
        "1000000000007 ff9f207ac14057ff febeffdfffefffff fffffffffffffffe";
    const CAPABILITIES_SLEEP_BUTTON: &str = "4000 0 0";
    const CAPABILITIES_VIDEO_BUS: &str = "3e000b00000000 0 0 0";
    const KEY_C: u16 = 46;
    const KEY_D: u16 = 32;

    fn matcher_for(accelerator: &str) -> Matcher {
        let mut matcher = Matcher::new();

        matcher.chord_set(chord_parse(accelerator).unwrap());

        matcher
    }

    fn event_of(kind: u16, code: u16, value: i32) -> [u8; EVENT_BYTES] {
        let mut event = [0_u8; EVENT_BYTES];

        event[16..18].copy_from_slice(&kind.to_ne_bytes());
        event[18..20].copy_from_slice(&code.to_ne_bytes());
        event[20..24].copy_from_slice(&value.to_ne_bytes());

        event
    }

    #[test]
    fn a_real_keyboard_bitmap_reads_as_a_keyboard_and_buttons_do_not() {
        assert!(capabilities_keyboard(CAPABILITIES_KEYBOARD));
        assert!(!capabilities_keyboard(CAPABILITIES_SLEEP_BUTTON));
        assert!(!capabilities_keyboard(CAPABILITIES_VIDEO_BUS));
        assert!(!capabilities_keyboard(""));
        assert!(!capabilities_keyboard("not hex at all"));
    }

    #[test]
    fn holding_the_chord_presses_once_and_letting_go_of_any_part_releases_once() {
        let mut matcher = matcher_for("Control+Shift+Space");

        assert_eq!(matcher.key_set(KEYS_CONTROL[0], true), None);
        assert_eq!(matcher.key_set(KEYS_SHIFT[1], true), None);
        assert_eq!(matcher.key_set(KEY_SPACE, true), Some(Transition::Pressed));
        assert_eq!(matcher.key_set(KEY_SPACE, true), None);
        assert_eq!(matcher.key_set(KEYS_SHIFT[1], false), Some(Transition::Released));
        assert_eq!(matcher.key_set(KEY_SPACE, false), None);
        assert_eq!(matcher.key_set(KEYS_CONTROL[0], false), None);
    }

    #[test]
    fn releasing_the_main_key_ends_the_chord_while_modifiers_stay_down() {
        let mut matcher = matcher_for("Control+Alt+KeyD");

        matcher.key_set(KEYS_CONTROL[1], true);
        matcher.key_set(KEYS_ALT[0], true);

        assert_eq!(matcher.key_set(KEY_D, true), Some(Transition::Pressed));
        assert_eq!(matcher.key_set(KEY_D, false), Some(Transition::Released));
        assert_eq!(matcher.key_set(KEY_D, true), Some(Transition::Pressed));
    }

    #[test]
    fn an_extra_modifier_or_a_different_key_never_presses() {
        let mut matcher = matcher_for("Control+Shift+Space");

        matcher.key_set(KEYS_CONTROL[0], true);
        matcher.key_set(KEYS_SHIFT[0], true);
        matcher.key_set(KEYS_ALT[0], true);

        assert_eq!(matcher.key_set(KEY_SPACE, true), None);

        matcher.key_set(KEY_SPACE, false);
        matcher.key_set(KEYS_ALT[0], false);

        assert_eq!(matcher.key_set(KEY_C, true), None);
        assert_eq!(matcher.key_set(KEY_SPACE, true), Some(Transition::Pressed));
    }

    #[test]
    fn a_function_key_alone_works_as_a_chord() {
        let mut matcher = matcher_for("F8");

        assert_eq!(matcher.key_set(66, true), Some(Transition::Pressed));
        assert_eq!(matcher.key_set(66, false), Some(Transition::Released));
    }

    #[test]
    fn changing_the_chord_mid_press_never_leaves_a_press_dangling() {
        let mut matcher = matcher_for("Control+Shift+Space");

        matcher.key_set(KEYS_CONTROL[0], true);
        matcher.key_set(KEYS_SHIFT[0], true);
        matcher.key_set(KEY_SPACE, true);
        matcher.chord_set(chord_parse("F9").unwrap());

        assert!(!matcher.active);
        assert_eq!(matcher.key_set(KEY_SPACE, false), None);
    }

    #[test]
    fn only_key_presses_and_releases_pass_and_repeats_are_dropped() {
        assert_eq!(event_key_of(&event_of(EVENT_KEY, KEY_SPACE, 1)), Some((KEY_SPACE, 1)));
        assert_eq!(event_key_of(&event_of(EVENT_KEY, KEY_SPACE, 0)), Some((KEY_SPACE, 0)));
        assert_eq!(event_key_of(&event_of(EVENT_KEY, KEY_SPACE, 2)), None);
        assert_eq!(event_key_of(&event_of(0, KEY_SPACE, 1)), None);
        assert_eq!(event_key_of(&event_of(EVENT_KEY, KEY_CODE_MAX + 1, 1)), None);
    }

    #[test]
    fn every_key_the_settings_page_records_maps_to_a_distinct_device_code() {
        let mut names: Vec<String> = Vec::new();

        names.extend((b'A'..=b'Z').map(|letter| format!("Key{}", char::from(letter))));
        names.extend((0..=9).map(|digit| format!("Digit{digit}")));
        names.extend((1..=24).map(|number| format!("F{number}")));
        names.extend((0..=9).map(|digit| format!("Numpad{digit}")));

        for name in [
            "Space",
            "Enter",
            "Tab",
            "Backspace",
            "Escape",
            "Minus",
            "Equal",
            "BracketLeft",
            "BracketRight",
            "Backslash",
            "Semicolon",
            "Quote",
            "Backquote",
            "Comma",
            "Period",
            "Slash",
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "Insert",
            "Delete",
            "Home",
            "End",
            "PageUp",
            "PageDown",
        ] {
            names.push(name.to_owned());
        }

        let mut seen = [0_u64; KEY_WORDS];

        for name in &names {
            let chord = chord_parse(&format!("Control+{name}")).unwrap();

            assert!(!key_bit(&seen, chord.key), "{name} shares a device code");

            seen[usize::from(chord.key.div_euclid(64))] |= 1 << (chord.key % 64);
        }

        assert_eq!(seen.iter().map(|word| word.count_ones()).sum::<u32>() as usize, names.len());
    }

    #[test]
    fn the_shortcut_letters_match_the_kernel_keyboard_layout() {
        assert_eq!(key_code_of(Code::KeyA), Some(KEY_A));
        assert_eq!(key_code_of(Code::KeyZ), Some(KEY_Z));
        assert_eq!(key_code_of(Code::KeyD), Some(KEY_D));
        assert_eq!(key_code_of(Code::Space), Some(KEY_SPACE));
        assert_eq!(key_code_of(Code::MediaPlayPause), None);
    }
}
