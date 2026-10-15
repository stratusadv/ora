use crate::error::{AppError, AppResult};
use crate::platform;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

pub(crate) const APP_DATA_FOLDER: &str = "com.stratusadv.ora";
pub(crate) const MODEL_CLEANUP_DEFAULT: &str = "stratus.turbo";
pub(crate) const SHORTCUT_DEFAULT: &str = "Control+Shift+Space";
const FILE_TEMPORARY_EXTENSION: &str = "tmp";
const HOST_CHARS_MAX: u32 = 2048;
const MICROPHONE_CHARS_MAX: u32 = 512;
const MODEL_CHARS_MAX: u32 = 128;
const SETTINGS_BYTES_MAX: u32 = 64 << 10;
const SETTINGS_FILE: &str = "settings.json";
const SHORTCUT_CHARS_MAX: u32 = 128;
const UTF8_BYTES_PER_CHAR_MAX: u32 = 4;
const VOCABULARY_CHARS_MAX: u32 = 2000;

const _: () = assert!(
    (HOST_CHARS_MAX
        + MICROPHONE_CHARS_MAX
        + MODEL_CHARS_MAX
        + SHORTCUT_CHARS_MAX
        + VOCABULARY_CHARS_MAX)
        * UTF8_BYTES_PER_CHAR_MAX
        * 2
        < SETTINGS_BYTES_MAX
);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PasteKeys {
    #[default]
    ControlV,
    ControlShiftV,
    ShiftInsert,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShortcutMode {
    Hold,
    #[default]
    HoldOrTap,
    Tap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Thinking {
    Low,
    Medium,
    Xhigh,
}

impl Thinking {
    pub(crate) const fn effort(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::Xhigh => "xhigh",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) api_host: Option<String>,
    pub(crate) cleanup_enabled: bool,
    pub(crate) cleanup_model: String,
    pub(crate) cleanup_thinking: Option<Thinking>,
    pub(crate) cues_enabled: bool,
    pub(crate) microphone: Option<String>,
    pub(crate) paste_keys: PasteKeys,
    pub(crate) shortcut: String,
    pub(crate) shortcut_mode: ShortcutMode,
    pub(crate) vocabulary: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_host: None,
            cleanup_enabled: true,
            cleanup_model: MODEL_CLEANUP_DEFAULT.to_owned(),
            cleanup_thinking: None,
            cues_enabled: true,
            microphone: None,
            paste_keys: PasteKeys::ControlV,
            shortcut: SHORTCUT_DEFAULT.to_owned(),
            shortcut_mode: ShortcutMode::HoldOrTap,
            vocabulary: String::new(),
        }
    }
}

impl Settings {
    pub(crate) fn normalized(self) -> AppResult<Self> {
        let api_host = text_optional(self.api_host.as_deref())
            .map(|host| host.trim_end_matches('/').to_owned())
            .filter(|host| !host.is_empty());

        let cleanup_model = text_or(&self.cleanup_model, MODEL_CLEANUP_DEFAULT);
        let microphone = text_optional(self.microphone.as_deref());
        let shortcut = text_or(&self.shortcut, SHORTCUT_DEFAULT);
        let vocabulary = self.vocabulary.trim().to_owned();

        text_bound("API address", api_host.as_deref().unwrap_or_default(), HOST_CHARS_MAX)?;
        text_bound("Cleanup model", &cleanup_model, MODEL_CHARS_MAX)?;
        text_bound("Microphone", microphone.as_deref().unwrap_or_default(), MICROPHONE_CHARS_MAX)?;
        text_bound("Shortcut", &shortcut, SHORTCUT_CHARS_MAX)?;
        text_bound("Vocabulary", &vocabulary, VOCABULARY_CHARS_MAX)?;

        if let Some(host) = api_host.as_deref() {
            host_validate(host)?;
        }

        let normalized = Self {
            api_host,
            cleanup_enabled: self.cleanup_enabled,
            cleanup_model,
            cleanup_thinking: self.cleanup_thinking,
            cues_enabled: self.cues_enabled,
            microphone,
            paste_keys: self.paste_keys,
            shortcut,
            shortcut_mode: self.shortcut_mode,
            vocabulary,
        };

        debug_assert_ne!(normalized.shortcut, "");
        debug_assert_ne!(normalized.cleanup_model, "");

        Ok(normalized)
    }
}

pub(crate) fn file_read_bounded(path: &Path, bytes_max: u32) -> AppResult<Option<Vec<u8>>> {
    debug_assert!(bytes_max > 0);

    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };

    let mut bytes = Vec::new();

    file.take(u64::from(bytes_max) + 1).read_to_end(&mut bytes)?;

    if bytes.len() > bytes_max as usize {
        return Err(AppError::Config(format!(
            "{} is larger than the {bytes_max} bytes ora reads",
            path.display()
        )));
    }

    debug_assert!(bytes.len() <= bytes_max as usize);

    Ok(Some(bytes))
}

pub(crate) fn file_write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let temporary = path.with_extension(FILE_TEMPORARY_EXTENSION);

    debug_assert!(path.is_absolute());
    debug_assert_ne!(temporary, path);

    {
        let mut file = fs::File::create(&temporary)?;

        file.write_all(bytes)?;
        file.sync_all()?;
    }

    fs::rename(&temporary, path)?;

    debug_assert!(path.exists());

    Ok(())
}

pub(crate) fn host_validate(host: &str) -> AppResult<()> {
    if host.is_empty() {
        return Err(AppError::Config(
            "No API address has been set. Enter one in ora's settings under Service.".into(),
        ));
    }

    let parsed = reqwest::Url::parse(host).map_err(|error| {
        AppError::Config(format!("The API address {host} is not a web address ({error})."))
    })?;

    if matches!(parsed.scheme(), "http" | "https") {
        return Ok(());
    }

    Err(AppError::Config(format!("The API address {host} must start with https:// or http://.")))
}

pub(crate) fn root_resolve() -> AppResult<PathBuf> {
    let data = dirs::data_dir().ok_or_else(|| {
        AppError::Config("this system reports no folder for application data".into())
    })?;

    let root = data.join(APP_DATA_FOLDER);

    platform::directory_private_create(&root)?;

    debug_assert!(root.is_absolute());
    debug_assert!(root.ends_with(APP_DATA_FOLDER));

    Ok(root)
}

pub(crate) fn settings_load(root: &Path) -> AppResult<Settings> {
    let path = root.join(SETTINGS_FILE);

    let Some(bytes) = file_read_bounded(&path, SETTINGS_BYTES_MAX)? else {
        return Ok(Settings::default());
    };

    let settings = serde_json::from_slice::<Settings>(&bytes)
        .map_err(AppError::from)
        .and_then(Settings::normalized);

    match settings {
        Ok(settings) => Ok(settings),
        Err(error) => {
            tracing::warn!("settings in {} were unusable, using defaults: {error}", path.display());

            Ok(Settings::default())
        }
    }
}

pub(crate) fn settings_save(root: &Path, settings: &Settings) -> AppResult<()> {
    let encoded = serde_json::to_vec_pretty(settings)?;

    if encoded.len() > SETTINGS_BYTES_MAX as usize {
        return Err(AppError::Config(format!(
            "the settings are {} bytes, over the {SETTINGS_BYTES_MAX} bytes ora keeps",
            encoded.len()
        )));
    }

    debug_assert!(encoded.starts_with(b"{"));
    debug_assert!(encoded.ends_with(b"}"));

    file_write_atomic(&root.join(SETTINGS_FILE), &encoded)
}

fn text_bound(label: &str, text: &str, chars_max: u32) -> AppResult<()> {
    let chars = text.chars().count();

    if chars > chars_max as usize {
        return Err(AppError::Config(format!(
            "{label} is {chars} characters, over the {chars_max} ora accepts."
        )));
    }

    Ok(())
}

fn text_optional(text: Option<&str>) -> Option<String> {
    text.map(str::trim).filter(|text| !text.is_empty()).map(str::to_owned)
}

fn text_or(text: &str, fallback: &str) -> String {
    debug_assert_ne!(fallback, "");

    let trimmed = text.trim();

    if trimmed.is_empty() {
        return fallback.to_owned();
    }

    trimmed.to_owned()
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};

    pub(crate) struct DirectoryScoped(PathBuf);

    impl DirectoryScoped {
        pub(crate) fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("ora-test-{name}-{}", std::process::id()));

            drop(std::fs::remove_dir_all(&path));
            std::fs::create_dir_all(&path).unwrap();

            Self(path)
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for DirectoryScoped {
        fn drop(&mut self) {
            drop(std::fs::remove_dir_all(&self.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::DirectoryScoped;
    use super::*;

    fn settings_custom() -> Settings {
        Settings {
            api_host: Some("https://api.example.test".to_owned()),
            cleanup_enabled: false,
            cleanup_model: "stratus.thinking".to_owned(),
            cleanup_thinking: Some(Thinking::Medium),
            cues_enabled: false,
            microphone: Some("pulseaudio:alsa_input.usb-yeti".to_owned()),
            paste_keys: PasteKeys::ShiftInsert,
            shortcut: "Control+Alt+KeyD".to_owned(),
            shortcut_mode: ShortcutMode::Tap,
            vocabulary: "Stratus, ora, Quartz Lantern".to_owned(),
        }
    }

    #[test]
    fn a_fresh_install_starts_with_cues_and_cleanup_on() {
        let settings = Settings::default();
        let directory = DirectoryScoped::new("settings-fresh");

        assert!(settings.cues_enabled);
        assert!(settings.cleanup_enabled);
        assert_eq!(settings_load(directory.path()).unwrap(), settings);
    }

    #[test]
    fn settings_survive_a_round_trip_through_the_disk() {
        let directory = DirectoryScoped::new("settings-round-trip");
        let settings = settings_custom();

        assert_eq!(settings_load(directory.path()).unwrap(), Settings::default());

        settings_save(directory.path(), &settings).unwrap();

        assert_eq!(settings_load(directory.path()).unwrap(), settings);
    }

    #[test]
    fn a_damaged_or_invalid_file_falls_back_to_defaults_and_an_oversized_one_is_refused() {
        let directory = DirectoryScoped::new("settings-damaged");
        let path = directory.path().join(SETTINGS_FILE);

        fs::write(&path, b"{ not json").unwrap();

        assert_eq!(settings_load(directory.path()).unwrap(), Settings::default());

        fs::write(&path, br#"{"api_host":"ftp://example.test"}"#).unwrap();

        assert_eq!(settings_load(directory.path()).unwrap(), Settings::default());

        fs::write(&path, vec![b' '; SETTINGS_BYTES_MAX as usize + 1]).unwrap();

        assert!(settings_load(directory.path()).is_err());
    }

    #[test]
    fn a_file_from_an_older_build_keeps_its_values_and_takes_defaults_for_the_rest() {
        let restored: Settings =
            serde_json::from_str(r#"{"cues_enabled":false,"unknown":1}"#).unwrap();

        assert!(!restored.cues_enabled);
        assert_eq!(restored.shortcut, SHORTCUT_DEFAULT);
        assert_eq!(restored.cleanup_model, MODEL_CLEANUP_DEFAULT);
        assert_eq!(restored.paste_keys, PasteKeys::ControlV);
        assert_eq!(restored.shortcut_mode, ShortcutMode::HoldOrTap);
    }

    #[test]
    fn normalizing_trims_blanks_to_defaults_and_strips_a_trailing_slash() {
        let raw = Settings {
            api_host: Some("  https://api.example.test/  ".to_owned()),
            cleanup_model: "   ".to_owned(),
            microphone: Some("  ".to_owned()),
            shortcut: String::new(),
            vocabulary: "  Stratus \n".to_owned(),
            ..Settings::default()
        };

        let normalized = raw.normalized().unwrap();

        assert_eq!(normalized.api_host.as_deref(), Some("https://api.example.test"));
        assert_eq!(normalized.cleanup_model, MODEL_CLEANUP_DEFAULT);
        assert_eq!(normalized.microphone, None);
        assert_eq!(normalized.shortcut, SHORTCUT_DEFAULT);
        assert_eq!(normalized.vocabulary, "Stratus");
        assert_eq!(normalized.clone().normalized().unwrap(), normalized);
    }

    #[test]
    fn normalizing_refuses_a_bad_address_and_text_past_its_limit_counted_in_characters() {
        let vocabulary_at_limit = "é".repeat(VOCABULARY_CHARS_MAX as usize);
        let vocabulary_past_limit = "e".repeat(VOCABULARY_CHARS_MAX as usize + 1);
        let host_bad =
            Settings { api_host: Some("example.test".to_owned()), ..Settings::default() };
        let host_ftp =
            Settings { api_host: Some("ftp://x.test".to_owned()), ..Settings::default() };
        let at_limit = Settings { vocabulary: vocabulary_at_limit, ..Settings::default() };
        let past_limit = Settings { vocabulary: vocabulary_past_limit, ..Settings::default() };

        assert!(host_bad.normalized().is_err());
        assert!(host_ftp.normalized().is_err());
        assert!(at_limit.normalized().is_ok());
        assert!(past_limit.normalized().is_err());
    }

    #[test]
    fn a_bounded_read_reports_a_missing_file_as_none_and_reads_up_to_its_limit() {
        let directory = DirectoryScoped::new("read-bounded");
        let path = directory.path().join("file.bin");

        assert!(file_read_bounded(&path, 4).unwrap().is_none());

        file_write_atomic(&path, b"abcd").unwrap();

        assert_eq!(file_read_bounded(&path, 4).unwrap().unwrap(), b"abcd");
        assert!(file_read_bounded(&path, 3).is_err());
        assert!(!path.with_extension(FILE_TEMPORARY_EXTENSION).exists());
    }

    #[test]
    fn the_settings_bound_holds_every_field_at_its_limit_with_escaping() {
        let settings = Settings {
            api_host: Some(format!("https://{}", "\u{1}".repeat(HOST_CHARS_MAX as usize - 8))),
            cleanup_model: "\u{1}".repeat(MODEL_CHARS_MAX as usize),
            microphone: Some("\u{1}".repeat(MICROPHONE_CHARS_MAX as usize)),
            shortcut: "\u{1}".repeat(SHORTCUT_CHARS_MAX as usize),
            vocabulary: "\u{1}".repeat(VOCABULARY_CHARS_MAX as usize),
            ..Settings::default()
        };

        let directory = DirectoryScoped::new("settings-bound");

        assert!(settings_save(directory.path(), &settings).is_ok());
    }
}
