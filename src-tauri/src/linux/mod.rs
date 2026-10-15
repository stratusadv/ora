mod keyboard;
mod portal;
mod uinput;
mod wayland;
mod x11;

use crate::error::{AppError, AppResult};
use crate::keystroke::{Chord, Key};
use crate::settings::{self, APP_DATA_FOLDER};
use crate::state::Backends;
use serde::Serialize;
use std::fs::{self, DirBuilder, File, Permissions};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::AppHandle;

const APPLICATIONS_FOLDER: &str = "applications";
const DESKTOP_FALLBACK: &str = "Linux";
const DIRECTORY_MODE_PRIVATE: u32 = 0o700;
const FILE_MODE_PRIVATE: u32 = 0o600;
const FILE_REFRESH_BYTES_MAX: u32 = 1 << 20;
const ICON_BYTES: &[u8] = include_bytes!("../../icons/128x128.png");
const ICON_FILE: &str = "icon.png";
const KEYSYM_CONTROL_LEFT: u32 = 0xffe3;
const KEYSYM_INSERT: u32 = 0xff63;
const KEYSYM_SHIFT_LEFT: u32 = 0xffe1;
const KEYSYM_V: u32 = 0x0076;
const SECRET_BYTES_MAX: u32 = 64 << 10;

const _: () = assert!(ICON_BYTES.len() < FILE_REFRESH_BYTES_MAX as usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KeystrokeBackend {
    Native,
    Portal,
    Uinput,
    Unavailable,
    VirtualKeyboard,
}

impl KeystrokeBackend {
    pub(crate) const fn typing(self) -> bool {
        !matches!(self, Self::Unavailable)
    }
}

#[derive(Debug)]
pub(crate) struct PlatformState {
    keyboard: keyboard::KeyboardState,
    uinput: Mutex<Option<File>>,
}

impl PlatformState {
    pub(crate) const fn new() -> Self {
        Self { keyboard: keyboard::KeyboardState::new(), uinput: Mutex::new(None) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShortcutBackend {
    Hotkey,
    Keyboard,
}

impl ShortcutBackend {
    pub(crate) const fn hotkey(self) -> bool {
        matches!(self, Self::Hotkey)
    }

    pub(crate) const fn keyboard(self) -> bool {
        matches!(self, Self::Keyboard)
    }
}

pub(crate) async fn backends_detect(platform: &PlatformState) -> Backends {
    if let Err(error) = desktop_entry_install() {
        tracing::warn!("the desktop entry was not written, so portals may refuse ora: {error}");
    }

    if !session_wayland() {
        return Backends { keystroke: KeystrokeBackend::Native, shortcut: ShortcutBackend::Hotkey };
    }

    portal::register().await;

    let keystroke = keystroke_detect(platform).await;

    debug_assert_ne!(keystroke, KeystrokeBackend::Native);

    Backends { keystroke, shortcut: ShortcutBackend::Keyboard }
}

pub(crate) async fn chord_send(
    platform: &PlatformState,
    backend: KeystrokeBackend,
    chord: Chord,
    root: &Path,
) -> AppResult<()> {
    debug_assert!(!chord.key.is_modifier());

    match backend {
        KeystrokeBackend::Native => x11::chord_send(chord),
        KeystrokeBackend::Portal => portal::chord_send(chord, root).await,
        KeystrokeBackend::Uinput => uinput::chord_send(&platform.uinput, chord),
        KeystrokeBackend::VirtualKeyboard => wayland::chord_send(chord),
        KeystrokeBackend::Unavailable => {
            Err(AppError::Keystroke("this desktop does not accept simulated keyboard input".into()))
        }
    }
}

pub(crate) fn desktop_name() -> String {
    let variable = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();

    desktop_name_of(&variable)
}

pub(crate) fn directory_private_create(path: &Path) -> AppResult<()> {
    DirBuilder::new().recursive(true).mode(DIRECTORY_MODE_PRIVATE).create(path)?;
    fs::set_permissions(path, Permissions::from_mode(DIRECTORY_MODE_PRIVATE))?;

    debug_assert!(path.is_dir());

    Ok(())
}

pub(crate) fn keyboard_chord_set(platform: &PlatformState, accelerator: &str) -> AppResult<()> {
    keyboard::chord_set(&platform.keyboard, accelerator)
}

pub(crate) fn keyboard_listen_start(app: &AppHandle) {
    debug_assert!(session_wayland());

    keyboard::listen_start(app);
}

pub(crate) const fn keysym_of(key: Key) -> u32 {
    match key {
        Key::Control => KEYSYM_CONTROL_LEFT,
        Key::Insert => KEYSYM_INSERT,
        Key::Shift => KEYSYM_SHIFT_LEFT,
        Key::V => KEYSYM_V,
    }
}

pub(crate) fn secret_read(path: &Path) -> AppResult<Option<String>> {
    let Some(bytes) = settings::file_read_bounded(path, SECRET_BYTES_MAX)? else {
        return Ok(None);
    };

    let text = String::from_utf8(bytes).map_err(|error| {
        AppError::Config(format!("{} does not hold text: {error}", path.display()))
    })?;

    Ok(Some(text))
}

pub(crate) fn secret_write(path: &Path, plaintext: &str) -> AppResult<()> {
    debug_assert_ne!(plaintext, "");

    settings::file_write_atomic(path, plaintext.as_bytes())?;
    fs::set_permissions(path, Permissions::from_mode(FILE_MODE_PRIVATE))?;

    debug_assert!(path.is_file());

    Ok(())
}

pub(crate) fn session_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty())
}

pub(crate) async fn typing_allow(root: &Path) -> AppResult<()> {
    portal::typing_allow(root).await
}

pub(crate) fn typing_allowed(root: &Path) -> bool {
    portal::token_exists(root)
}

fn desktop_entry_install() -> AppResult<()> {
    let root = settings::root_resolve()?;

    let data = dirs::data_dir().ok_or_else(|| {
        AppError::Config("this system reports no folder for application data".into())
    })?;

    let applications = data.join(APPLICATIONS_FOLDER);
    let entry = applications.join(format!("{APP_DATA_FOLDER}.desktop"));
    let icon = root.join(ICON_FILE);

    let executable = std::env::var_os("APPIMAGE")
        .map_or_else(std::env::current_exe, |path| Ok(PathBuf::from(path)))?;

    fs::create_dir_all(&applications)?;
    file_refresh(&icon, ICON_BYTES)?;
    file_refresh(&entry, desktop_entry_text(&executable, &icon).as_bytes())?;

    debug_assert!(entry.is_file());

    Ok(())
}

fn desktop_entry_text(executable: &Path, icon: &Path) -> String {
    let exec = desktop_exec_quote(&executable.to_string_lossy());

    let text = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=ora\n\
         Comment=Dictation into any app\n\
         Exec={exec}\n\
         Icon={}\n\
         Terminal=false\n\
         NoDisplay=true\n\
         StartupWMClass={APP_DATA_FOLDER}\n",
        icon.display()
    );

    debug_assert!(text.starts_with("[Desktop Entry]"));

    text
}

fn desktop_exec_quote(path: &str) -> String {
    let mut quoted = String::with_capacity(path.len() + 2);

    quoted.push('"');

    for character in path.chars() {
        match character {
            '\\' => quoted.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                quoted.push('\\');
                quoted.push(character);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(character),
        }
    }

    quoted.push('"');

    debug_assert!(quoted.len() >= path.len() + 2);

    quoted
}

fn desktop_name_of(variable: &str) -> String {
    let name = variable.split(':').map(str::trim).rfind(|part| !part.is_empty());

    name.unwrap_or(DESKTOP_FALLBACK).to_owned()
}

fn file_refresh(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let current = settings::file_read_bounded(path, FILE_REFRESH_BYTES_MAX)?;

    if current.as_deref() == Some(bytes) {
        return Ok(());
    }

    settings::file_write_atomic(path, bytes)
}

async fn keystroke_detect(platform: &PlatformState) -> KeystrokeBackend {
    if wayland::virtual_keyboard_available() {
        return KeystrokeBackend::VirtualKeyboard;
    }

    if uinput::open(&platform.uinput) {
        return KeystrokeBackend::Uinput;
    }

    if portal::remote_desktop_available().await {
        return KeystrokeBackend::Portal;
    }

    KeystrokeBackend::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysyms_match_the_x11_keysym_table() {
        assert_eq!(keysym_of(Key::V), u32::from('v'));
        assert_eq!(keysym_of(Key::Control), 0xffe3);
        assert_eq!(keysym_of(Key::Shift), 0xffe1);
        assert_eq!(keysym_of(Key::Insert), 0xff63);
    }

    #[test]
    fn a_desktop_name_is_the_last_part_of_the_desktop_variable() {
        assert_eq!(desktop_name_of("ubuntu:GNOME"), "GNOME");
        assert_eq!(desktop_name_of("KDE"), "KDE");
        assert_eq!(desktop_name_of("Hyprland"), "Hyprland");
        assert_eq!(desktop_name_of("GNOME: "), "GNOME");
        assert_eq!(desktop_name_of(""), DESKTOP_FALLBACK);
    }

    #[test]
    fn an_exec_path_is_quoted_and_escaped_as_the_desktop_entry_specification_requires() {
        assert_eq!(desktop_exec_quote("/usr/bin/ora"), r#""/usr/bin/ora""#);
        assert_eq!(desktop_exec_quote("/home/a b/ora"), r#""/home/a b/ora""#);
        assert_eq!(desktop_exec_quote("/opt/$x/o`r\"a"), r#""/opt/\$x/o\`r\"a""#);
        assert_eq!(desktop_exec_quote(r"/opt/a\b"), r#""/opt/a\\\\b""#);
        assert_eq!(desktop_exec_quote("/opt/100%/ora"), r#""/opt/100%%/ora""#);
    }

    #[test]
    fn a_desktop_entry_is_hidden_and_names_the_portal_app_id() {
        let text = desktop_entry_text(Path::new("/opt/ora/ora"), Path::new("/data/icon.png"));

        assert!(text.contains("\nExec=\"/opt/ora/ora\"\n"));
        assert!(text.contains("\nIcon=/data/icon.png\n"));
        assert!(text.contains("\nNoDisplay=true\n"));
        assert!(text.contains(&format!("\nStartupWMClass={APP_DATA_FOLDER}\n")));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn only_an_unavailable_backend_refuses_to_type() {
        assert!(KeystrokeBackend::Native.typing());
        assert!(KeystrokeBackend::Portal.typing());
        assert!(KeystrokeBackend::Uinput.typing());
        assert!(KeystrokeBackend::VirtualKeyboard.typing());
        assert!(!KeystrokeBackend::Unavailable.typing());
        assert!(ShortcutBackend::Hotkey.hotkey());
        assert!(ShortcutBackend::Keyboard.keyboard());
        assert!(!ShortcutBackend::Keyboard.hotkey());
        assert!(!ShortcutBackend::Hotkey.keyboard());
    }
}
