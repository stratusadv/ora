use crate::error::{AppError, AppResult};
use crate::keystroke::{Chord, Key};
use crate::settings;
use crate::state::Backends;
use serde::Serialize;
use std::fs;
use std::future::{self, Future};
use std::path::Path;
use std::ptr;
use tauri::AppHandle;
use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB,
    CRYPTPROTECT_UI_FORBIDDEN,
    CryptProtectData,
    CryptUnprotectData,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT,
    INPUT_0,
    INPUT_KEYBOARD,
    KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP,
    SendInput,
    VIRTUAL_KEY,
    VK_CONTROL,
    VK_INSERT,
    VK_SHIFT,
    VK_V,
};

const CHORD_INPUTS_MAX: usize = 8;
const DESKTOP_NAME: &str = "Windows";
const SECRET_BYTES_MAX: u32 = 64 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KeystrokeBackend {
    Native,
}

impl KeystrokeBackend {
    pub(crate) const fn typing(self) -> bool {
        match self {
            Self::Native => true,
        }
    }
}

#[derive(Debug)]
pub(crate) struct PlatformState;

impl PlatformState {
    pub(crate) const fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShortcutBackend {
    Hotkey,
}

impl ShortcutBackend {
    pub(crate) const fn hotkey(self) -> bool {
        match self {
            Self::Hotkey => true,
        }
    }

    pub(crate) const fn keyboard(self) -> bool {
        match self {
            Self::Hotkey => false,
        }
    }
}

pub(crate) fn backends_detect(_platform: &PlatformState) -> impl Future<Output = Backends> {
    future::ready(Backends {
        keystroke: KeystrokeBackend::Native,
        shortcut: ShortcutBackend::Hotkey,
    })
}

pub(crate) fn chord_send(
    _platform: &PlatformState,
    backend: KeystrokeBackend,
    chord: Chord,
    _root: &Path,
) -> impl Future<Output = AppResult<()>> {
    let sent = match backend {
        KeystrokeBackend::Native => inputs_send(chord),
    };

    future::ready(sent)
}

pub(crate) fn desktop_name() -> String {
    DESKTOP_NAME.to_owned()
}

pub(crate) fn directory_private_create(path: &Path) -> AppResult<()> {
    fs::create_dir_all(path)?;

    debug_assert!(path.is_dir());

    Ok(())
}

pub(crate) fn keyboard_chord_set(_platform: &PlatformState, _accelerator: &str) -> AppResult<()> {
    Err(AppError::Shortcut("reading the keyboard directly is only for Linux Wayland".into()))
}

pub(crate) const fn keyboard_listen_start(_app: &AppHandle) {}

pub(crate) fn secret_read(path: &Path) -> AppResult<Option<String>> {
    let Some(sealed) = settings::file_read_bounded(path, SECRET_BYTES_MAX)? else {
        return Ok(None);
    };

    let plaintext = protect_reverse(&sealed)?;

    let text = String::from_utf8(plaintext).map_err(|error| {
        AppError::Config(format!("{} did not decrypt to text: {error}", path.display()))
    })?;

    Ok(Some(text))
}

pub(crate) fn secret_write(path: &Path, plaintext: &str) -> AppResult<()> {
    debug_assert_ne!(plaintext, "");

    let sealed = protect(plaintext.as_bytes())?;

    debug_assert_ne!(sealed.as_slice(), plaintext.as_bytes());

    settings::file_write_atomic(path, &sealed)
}

pub(crate) const fn session_wayland() -> bool {
    false
}

pub(crate) fn typing_allow(_root: &Path) -> impl Future<Output = AppResult<()>> {
    future::ready(Ok(()))
}

pub(crate) const fn typing_allowed(_root: &Path) -> bool {
    true
}

fn blob_input(bytes: &[u8]) -> AppResult<CRYPT_INTEGER_BLOB> {
    let length = u32::try_from(bytes.len())
        .map_err(|_overflow| AppError::Config("the secret is larger than DPAPI accepts".into()))?;

    Ok(CRYPT_INTEGER_BLOB { cbData: length, pbData: bytes.as_ptr().cast_mut() })
}

fn blob_output_take(blob: &CRYPT_INTEGER_BLOB, call: &str) -> AppResult<Vec<u8>> {
    if blob.pbData.is_null() {
        return Err(AppError::Config(format!("{call} returned no data")));
    }

    let length = blob.cbData as usize;

    // SAFETY: DPAPI returned a LocalAlloc buffer of `cbData` bytes at `pbData`, checked non-null.
    let bytes = unsafe { std::slice::from_raw_parts(blob.pbData, length) }.to_vec();

    // SAFETY: The buffer came from DPAPI, is freed exactly once, and is not read afterwards.
    let freed = unsafe { LocalFree(blob.pbData.cast()) };

    debug_assert!(freed.is_null());
    debug_assert_eq!(bytes.len(), length);

    Ok(bytes)
}

fn error_last(call: &str) -> AppError {
    // SAFETY: GetLastError only reads the calling thread's last error value.
    let code = unsafe { GetLastError() };

    AppError::Config(format!("{call} failed (code {code})"))
}

const fn input_key(key: Key, release: bool) -> INPUT {
    let (virtual_key, extended) = key_virtual(key);
    let mut flags = 0;

    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }

    if release {
        flags |= KEYEVENTF_KEYUP;
    }

    let keyboard =
        KEYBDINPUT { wVk: virtual_key, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 };

    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: keyboard } }
}

fn inputs_build(chord: Chord) -> Vec<INPUT> {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(CHORD_INPUTS_MAX);

    for modifier in chord.modifiers {
        inputs.push(input_key(*modifier, false));
    }

    inputs.push(input_key(chord.key, false));
    inputs.push(input_key(chord.key, true));

    for modifier in chord.modifiers.iter().rev() {
        inputs.push(input_key(*modifier, true));
    }

    debug_assert!(inputs.len() <= CHORD_INPUTS_MAX);
    debug_assert_eq!(inputs.len(), 2 * (chord.modifiers.len() + 1));

    inputs
}

fn inputs_send(chord: Chord) -> AppResult<()> {
    let inputs = inputs_build(chord);

    let count = u32::try_from(inputs.len())
        .map_err(|_overflow| AppError::Keystroke("too many keys in one chord".into()))?;

    let size = i32::try_from(size_of::<INPUT>())
        .map_err(|_overflow| AppError::Keystroke("the INPUT structure is too large".into()))?;

    // SAFETY: `inputs` holds `count` initialized INPUT values and outlives the call.
    let sent = unsafe { SendInput(count, inputs.as_ptr(), size) };

    if sent != count {
        return Err(AppError::Keystroke(
            "Windows blocked the paste keys. The window you dictated into may be running as \
             administrator, which ora cannot type into; the text is on the clipboard."
                .into(),
        ));
    }

    Ok(())
}

const fn key_virtual(key: Key) -> (VIRTUAL_KEY, bool) {
    match key {
        Key::Control => (VK_CONTROL, false),
        Key::Insert => (VK_INSERT, true),
        Key::Shift => (VK_SHIFT, false),
        Key::V => (VK_V, false),
    }
}

fn protect(plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let input = blob_input(plaintext)?;
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };

    // SAFETY: Both blobs are valid for the call, and DPAPI allocates the output buffer.
    let succeeded = unsafe {
        CryptProtectData(
            &raw const input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    };

    if succeeded == 0 {
        return Err(error_last("CryptProtectData"));
    }

    blob_output_take(&output, "CryptProtectData")
}

fn protect_reverse(sealed: &[u8]) -> AppResult<Vec<u8>> {
    let input = blob_input(sealed)?;
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };

    // SAFETY: Both blobs are valid for the call, and DPAPI allocates the output buffer.
    let succeeded = unsafe {
        CryptUnprotectData(
            &raw const input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    };

    if succeeded == 0 {
        return Err(error_last("CryptUnprotectData"));
    }

    blob_output_take(&output, "CryptUnprotectData")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystroke::chord_for;
    use crate::settings::PasteKeys;
    use crate::settings::test_support::DirectoryScoped;

    fn keyboard_of(input: &INPUT) -> (VIRTUAL_KEY, u32) {
        assert_eq!(input.r#type, INPUT_KEYBOARD);

        // SAFETY: The input was built as a keyboard event, so `ki` is the initialized variant.
        let keyboard = unsafe { input.Anonymous.ki };

        (keyboard.wVk, keyboard.dwFlags)
    }

    #[test]
    fn a_chord_presses_its_modifiers_first_and_releases_them_last() {
        let inputs = inputs_build(chord_for(PasteKeys::ControlShiftV));
        let actual: Vec<(VIRTUAL_KEY, u32)> = inputs.iter().map(keyboard_of).collect();

        let expected = [
            (VK_CONTROL, 0),
            (VK_SHIFT, 0),
            (VK_V, 0),
            (VK_V, KEYEVENTF_KEYUP),
            (VK_SHIFT, KEYEVENTF_KEYUP),
            (VK_CONTROL, KEYEVENTF_KEYUP),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn the_insert_key_is_sent_as_the_extended_navigation_key() {
        let inputs = inputs_build(chord_for(PasteKeys::ShiftInsert));
        let actual: Vec<(VIRTUAL_KEY, u32)> = inputs.iter().map(keyboard_of).collect();

        let expected = [
            (VK_SHIFT, 0),
            (VK_INSERT, KEYEVENTF_EXTENDEDKEY),
            (VK_INSERT, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
            (VK_SHIFT, KEYEVENTF_KEYUP),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn a_secret_seals_on_disk_and_opens_back_to_the_same_text() {
        let directory = DirectoryScoped::new("dpapi");
        let path = directory.path().join("secret.bin");

        secret_write(&path, "user-key").unwrap();

        assert_ne!(fs::read(&path).unwrap(), b"user-key");
        assert_eq!(secret_read(&path).unwrap().as_deref(), Some("user-key"));
    }
}
