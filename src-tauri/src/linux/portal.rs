use crate::error::{AppError, AppResult};
use crate::keystroke::{Chord, Key};
use crate::settings::{self, APP_DATA_FOLDER};
use ashpd::desktop::remote_desktop::{
    DeviceType,
    KeyState,
    NotifyKeyboardKeysymOptions,
    RemoteDesktop,
    SelectDevicesOptions,
    StartOptions,
};
use ashpd::desktop::{CreateSessionOptions, PersistMode, Session};
use ashpd::enumflags2::BitFlags;
use std::path::Path;
use std::time::Duration;

const REGISTER_ATTEMPTS_MAX: u32 = 3;
const REGISTER_RETRY_MS: u64 = 500;
const START_TIMEOUT_SECONDS: u64 = 120;
const TOKEN_BYTES_MAX: u32 = 4096;
const TOKEN_FILE: &str = "remote_desktop_token";

pub(super) async fn chord_send(chord: Chord, root: &Path) -> AppResult<()> {
    let proxy = RemoteDesktop::new().await.map_err(keystroke_error)?;
    let session = session_start(&proxy, root).await?;
    let pressed = chord_press(&proxy, &session, chord).await;

    keys_release(&proxy, &session, chord).await;

    if let Err(error) = session.close().await {
        tracing::warn!("the remote desktop session did not close: {error}");
    }

    pressed
}

pub(super) async fn register() {
    let app_id = match ashpd::AppID::try_from(APP_DATA_FOLDER) {
        Ok(app_id) => app_id,
        Err(error) => {
            tracing::warn!("{APP_DATA_FOLDER} is not a valid portal app id: {error}");

            return;
        }
    };

    for attempt in 1..=REGISTER_ATTEMPTS_MAX {
        let Err(error) = ashpd::register_host_app(app_id.clone()).await else {
            return;
        };

        tracing::info!("the portal registry did not take the app id on attempt {attempt}: {error}");
        tokio::time::sleep(Duration::from_millis(REGISTER_RETRY_MS)).await;
    }
}

pub(super) async fn remote_desktop_available() -> bool {
    let proxy = match RemoteDesktop::new().await {
        Ok(proxy) => proxy,
        Err(error) => {
            tracing::info!("the remote desktop portal is unavailable: {error}");

            return false;
        }
    };

    match proxy.available_device_types().await {
        Ok(types) => types.contains(DeviceType::Keyboard),
        Err(error) => {
            tracing::info!("the remote desktop portal lists no device types: {error}");

            false
        }
    }
}

pub(super) fn token_exists(root: &Path) -> bool {
    token_read(root).is_some()
}

pub(super) async fn typing_allow(root: &Path) -> AppResult<()> {
    let proxy = RemoteDesktop::new().await.map_err(keystroke_error)?;
    let session = session_start(&proxy, root).await?;

    session.close().await.map_err(keystroke_error)?;

    debug_assert!(token_exists(root));

    Ok(())
}

async fn chord_press(
    proxy: &RemoteDesktop,
    session: &Session<RemoteDesktop>,
    chord: Chord,
) -> AppResult<()> {
    for modifier in chord.modifiers {
        keysym_notify(proxy, session, *modifier, KeyState::Pressed).await?;
    }

    keysym_notify(proxy, session, chord.key, KeyState::Pressed).await
}

fn keystroke_error(error: impl std::fmt::Display) -> AppError {
    AppError::Keystroke(format!("the desktop refused to type for ora: {error}"))
}

async fn keys_release(proxy: &RemoteDesktop, session: &Session<RemoteDesktop>, chord: Chord) {
    let keys = std::iter::once(&chord.key).chain(chord.modifiers.iter().rev());

    for key in keys {
        if let Err(error) = keysym_notify(proxy, session, *key, KeyState::Released).await {
            tracing::warn!("the {key:?} key may not have been released: {error}");
        }
    }
}

async fn keysym_notify(
    proxy: &RemoteDesktop,
    session: &Session<RemoteDesktop>,
    key: Key,
    state: KeyState,
) -> AppResult<()> {
    let keysym = i32::try_from(super::keysym_of(key)).map_err(|error| {
        AppError::Keystroke(format!("the {key:?} keysym is out of range: {error}"))
    })?;

    proxy
        .notify_keyboard_keysym(session, keysym, state, NotifyKeyboardKeysymOptions::default())
        .await
        .map_err(keystroke_error)
}

async fn session_prepare(
    proxy: &RemoteDesktop,
    session: &Session<RemoteDesktop>,
    root: &Path,
) -> AppResult<()> {
    let token = token_read(root);

    let options = SelectDevicesOptions::default()
        .set_devices(BitFlags::from_flag(DeviceType::Keyboard))
        .set_persist_mode(PersistMode::ExplicitlyRevoked)
        .set_restore_token(token.as_deref());

    proxy
        .select_devices(session, options)
        .await
        .map_err(keystroke_error)?
        .response()
        .map_err(keystroke_error)?;

    let started = tokio::time::timeout(Duration::from_secs(START_TIMEOUT_SECONDS), async {
        proxy.start(session, None, StartOptions::default()).await?.response()
    })
    .await
    .map_err(|_elapsed| {
        AppError::Keystroke("the keyboard input permission request was not answered".into())
    })?
    .map_err(keystroke_error)?;

    if let Some(token) = started.restore_token() {
        token_write(root, token)?;
    }

    if !started.devices().contains(DeviceType::Keyboard) {
        return Err(AppError::Keystroke("keyboard input permission was not granted".into()));
    }

    Ok(())
}

async fn session_start(proxy: &RemoteDesktop, root: &Path) -> AppResult<Session<RemoteDesktop>> {
    let session =
        proxy.create_session(CreateSessionOptions::default()).await.map_err(keystroke_error)?;
    let prepared = session_prepare(proxy, &session, root).await;

    if let Err(error) = prepared {
        if let Err(close_error) = session.close().await {
            tracing::warn!("the refused remote desktop session did not close: {close_error}");
        }

        return Err(error);
    }

    Ok(session)
}

fn token_read(root: &Path) -> Option<String> {
    let path = root.join(TOKEN_FILE);

    let bytes = match settings::file_read_bounded(&path, TOKEN_BYTES_MAX) {
        Ok(bytes) => bytes?,
        Err(error) => {
            tracing::warn!("the remote desktop token could not be read: {error}");

            return None;
        }
    };

    String::from_utf8(bytes)
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

fn token_write(root: &Path, token: &str) -> AppResult<()> {
    if token.len() > TOKEN_BYTES_MAX as usize {
        return Err(AppError::Keystroke("the desktop returned an oversized token".into()));
    }

    super::secret_write(&root.join(TOKEN_FILE), token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::test_support::DirectoryScoped;

    #[test]
    fn a_stored_token_reads_back_trimmed_and_a_blank_one_counts_as_absent() {
        let directory = DirectoryScoped::new("portal-token");

        assert!(!token_exists(directory.path()));

        token_write(directory.path(), "restore-token\n").unwrap();

        assert_eq!(token_read(directory.path()).as_deref(), Some("restore-token"));

        std::fs::write(directory.path().join(TOKEN_FILE), b"  \n").unwrap();

        assert!(!token_exists(directory.path()));
    }

    #[test]
    fn an_oversized_token_is_refused_before_it_is_written() {
        let directory = DirectoryScoped::new("portal-token-oversized");
        let token = "t".repeat(TOKEN_BYTES_MAX as usize + 1);

        assert!(token_write(directory.path(), &token).is_err());
        assert!(!token_exists(directory.path()));
    }
}
