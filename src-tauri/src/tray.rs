use crate::dictation::Phase;
use crate::error::{AppError, AppResult};
use crate::monitor;
use crate::settings::{self, Theme};
use crate::state::AppState;
use std::time::Instant;
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::utils::config::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub(crate) const WINDOW_MAIN: &str = "main";
const WINDOW_BACKGROUND_DARK: Color = Color(8, 9, 10, 255);
const WINDOW_BACKGROUND_LIGHT: Color = Color(247, 247, 248, 255);
const WINDOW_HEIGHT: f64 = 660.0;
const WINDOW_HEIGHT_MIN: f64 = 660.0;
const WINDOW_TITLE: &str = "ora";
const WINDOW_WIDTH: f64 = 560.0;
const WINDOW_WIDTH_MIN: f64 = 460.0;
const COLOR_LISTENING: [u8; 4] = [229, 72, 77, 255];
const COLOR_TRANSCRIBING: [u8; 4] = [245, 165, 36, 255];
const MARK_CUTOUT: [u8; 4] = [0, 0, 0, 0];
const MARK_RADIUS_PERCENT: i64 = 22;
const MARK_RING_PERCENT: i64 = 6;
const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";
const PERCENT_WHOLE: i64 = 100;
const PIXEL_BYTES: u32 = 4;
const TOOLTIP_IDLE: &str = "ora: ready";
const TOOLTIP_LISTENING: &str = "ora: recording";
const TOOLTIP_TRANSCRIBING: &str = "ora: transcribing";
const TRAY_ID: &str = "ora";

const _: () = assert!(MARK_RADIUS_PERCENT + MARK_RING_PERCENT < PERCENT_WHOLE >> 1);

#[derive(Debug, Clone)]
pub(crate) struct TrayIcons {
    idle: Image<'static>,
    listening: Image<'static>,
    transcribing: Image<'static>,
}

pub(crate) fn tray_create(app: &AppHandle) -> AppResult<()> {
    let base = app
        .default_window_icon()
        .ok_or_else(|| AppError::Config("the app icon is missing from this build".into()))?
        .clone()
        .to_owned();

    let icons = TrayIcons {
        idle: base.clone(),
        listening: icon_marked(&base, COLOR_LISTENING),
        transcribing: icon_marked(&base, COLOR_TRANSCRIBING),
    };

    let open =
        MenuItem::with_id(app, MENU_OPEN, "Open ora", true, None::<&str>).map_err(tray_error)?;
    let quit =
        MenuItem::with_id(app, MENU_QUIT, "Quit ora", true, None::<&str>).map_err(tray_error)?;
    let menu = Menu::with_items(app, &[&open, &quit]).map_err(tray_error)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icons.idle.clone())
        .tooltip(TOOLTIP_IDLE)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| tray_menu_handle(app, &event))
        .on_tray_icon_event(|tray, event| tray_icon_handle(tray, &event))
        .build(app)
        .map_err(tray_error)?;

    let stored = app.state::<AppState>().tray_icons.set(icons).is_ok();

    debug_assert!(stored);

    Ok(())
}

pub(crate) fn tray_phase_show(app: &AppHandle, phase: Phase) {
    let state = app.state::<AppState>();

    let (Some(icons), Some(tray)) = (state.tray_icons.get(), app.tray_by_id(TRAY_ID)) else {
        return;
    };

    let (icon, tooltip) = match phase {
        Phase::Idle => (&icons.idle, TOOLTIP_IDLE),
        Phase::Listening => (&icons.listening, TOOLTIP_LISTENING),
        Phase::Transcribing => (&icons.transcribing, TOOLTIP_TRANSCRIBING),
    };

    if let Err(error) = tray.set_icon(Some(icon.clone())) {
        tracing::warn!("the tray icon did not change: {error}");
    }

    if let Err(error) = tray.set_tooltip(Some(tooltip)) {
        tracing::debug!("the tray tooltip did not change: {error}");
    }
}

pub(crate) fn window_main_show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW_MAIN) else {
        let state = app.state::<AppState>();

        if let Ok(mut started) = state.window_build_started.lock() {
            *started = Some(Instant::now());
        }

        if let Err(error) = window_main_build(app) {
            tracing::warn!("the settings window could not be built: {error}");
        }

        return;
    };

    window_reveal(&window);
}

pub(crate) fn window_reveal(window: &WebviewWindow) {
    let state = window.state::<AppState>();
    let started = state.window_build_started.lock().ok().and_then(|mut started| started.take());

    if let Some(started) = started {
        tracing::info!(
            "the window was ready {} ms after it was opened",
            started.elapsed().as_millis()
        );
    }

    if let Err(error) = window.unminimize() {
        tracing::debug!("the settings window did not unminimize: {error}");
    }

    if let Err(error) = window.show() {
        tracing::warn!("the settings window did not show: {error}");
    }

    monitor::monitor_resume(window.app_handle());

    if let Err(error) = window.set_focus() {
        tracing::debug!("the settings window did not take focus: {error}");
    }
}

pub(crate) fn window_theme_apply(window: &WebviewWindow, theme: Option<Theme>) {
    let requested = theme.map(|theme| match theme {
        Theme::Dark => tauri::Theme::Dark,
        Theme::Light => tauri::Theme::Light,
    });

    // None hands the titlebar back to the desktop, so "Match my computer" keeps following it.
    if let Err(error) = window.set_theme(requested) {
        tracing::debug!("the settings window did not take the theme: {error}");
    }

    let effective = requested.or_else(|| window.theme().ok());
    let background = window_background_of(effective);

    debug_assert!(background == WINDOW_BACKGROUND_DARK || background == WINDOW_BACKGROUND_LIGHT);

    if let Err(error) = window.set_background_color(Some(background)) {
        tracing::debug!("the settings window did not take the background: {error}");
    }
}

fn window_background_of(theme: Option<tauri::Theme>) -> Color {
    if theme == Some(tauri::Theme::Light) {
        return WINDOW_BACKGROUND_LIGHT;
    }

    WINDOW_BACKGROUND_DARK
}

fn window_main_build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, WINDOW_MAIN, WebviewUrl::App("index.html".into()))
        .title(WINDOW_TITLE)
        .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .min_inner_size(WINDOW_WIDTH_MIN, WINDOW_HEIGHT_MIN)
        .center()
        .visible(false)
        .background_color(WINDOW_BACKGROUND_DARK)
        .build()?;

    window_theme_apply(&window, settings::settings_current().theme);

    Ok(window)
}

fn icon_marked(base: &Image<'_>, color: [u8; 4]) -> Image<'static> {
    let width = i64::from(base.width());
    let size = width.min(i64::from(base.height()));
    let radius = (size * MARK_RADIUS_PERCENT).div_euclid(PERCENT_WHOLE).max(1);
    let ring = radius + (size * MARK_RING_PERCENT).div_euclid(PERCENT_WHOLE).max(1);
    let center_x = width - ring - 1;
    let center_y = i64::from(base.height()) - ring - 1;
    let mut rgba = base.rgba().to_vec();

    debug_assert_eq!(rgba.len(), base.rgba().len());
    debug_assert!(rgba.len().is_multiple_of(PIXEL_BYTES as usize));

    for (index, pixel) in (0_i64..).zip(rgba.as_chunks_mut::<{ PIXEL_BYTES as usize }>().0) {
        let offset_x = index.rem_euclid(width) - center_x;
        let offset_y = index.div_euclid(width) - center_y;
        let distance = offset_x * offset_x + offset_y * offset_y;

        if distance <= radius * radius {
            pixel.copy_from_slice(&color);

            continue;
        }

        if distance <= ring * ring {
            pixel.copy_from_slice(&MARK_CUTOUT);
        }
    }

    Image::new_owned(rgba, base.width(), base.height())
}

fn tray_error(error: impl std::fmt::Display) -> AppError {
    AppError::Config(format!("the tray icon could not be set up: {error}"))
}

fn tray_icon_handle(tray: &TrayIcon, event: &TrayIconEvent) {
    let TrayIconEvent::Click { button, button_state, .. } = *event else {
        return;
    };

    if button != MouseButton::Left {
        return;
    }

    if button_state != MouseButtonState::Up {
        return;
    }

    window_main_show(tray.app_handle());
}

fn tray_menu_handle(app: &AppHandle, event: &MenuEvent) {
    match event.id().as_ref() {
        MENU_OPEN => window_main_show(app),
        MENU_QUIT => app.exit(0),
        other => tracing::warn!("the tray menu sent an unknown item: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &Image<'_>, column: u32, row: u32) -> [u8; 4] {
        let index = ((row * image.width() + column) * 4) as usize;

        image.rgba()[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn only_a_light_theme_gets_the_light_background() {
        assert_eq!(window_background_of(Some(tauri::Theme::Light)), WINDOW_BACKGROUND_LIGHT);
        assert_eq!(window_background_of(Some(tauri::Theme::Dark)), WINDOW_BACKGROUND_DARK);
        assert_eq!(window_background_of(None), WINDOW_BACKGROUND_DARK);
    }

    #[test]
    fn a_marked_icon_paints_a_dot_in_its_corner_and_leaves_the_rest() {
        let base = Image::new_owned(vec![9; 64 * 64 * 4], 64, 64);
        let marked = icon_marked(&base, COLOR_LISTENING);

        assert_eq!(marked.width(), 64);
        assert_eq!(marked.height(), 64);
        assert_eq!(pixel(&marked, 0, 0), [9, 9, 9, 9]);
        assert_eq!(pixel(&marked, 63, 0), [9, 9, 9, 9]);
        assert_eq!(pixel(&marked, 48, 48), COLOR_LISTENING);
        assert_eq!(pixel(&marked, 63, 63), [9, 9, 9, 9]);
        assert!(marked.rgba().as_chunks::<4>().0.contains(&MARK_CUTOUT));
    }
}
