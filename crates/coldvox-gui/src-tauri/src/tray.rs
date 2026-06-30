//! System tray for the ColdVox overlay shell.
//!
//! Ported from Handy (`cjpais/handy` → `src-tauri/src/tray.rs`) and adapted to
//! ColdVox's [`OverlayStatus`](crate::contract::OverlayStatus) lifecycle.
//!
//! Unlike Handy, ColdVox's GUI crate does not own a model/history manager, so
//! the tray is intentionally lean: it reflects pipeline status in the tooltip,
//! offers Start/Stop/Pause/Clear/Quit, and routes every click through a single
//! Tauri event ([`TRAY_EVENT_NAME`]) that `lib.rs` dispatches to the same
//! command bodies the React overlay uses. That keeps a single source of truth
//! for pipeline lifecycle regardless of whether the user clicks the tray or
//! the overlay buttons.
//!
//! Per-state tray icons (idle/listening/processing) are intentionally deferred
//! — the first port reuses the app's default window icon and varies the tooltip
//! + menu instead. Swapping in a state icon set is a follow-up.

use crate::contract::OverlayStatus;
use log::warn;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager,
};

/// Event name emitted to the app-level listener in `lib.rs` whenever a tray
/// menu item is activated. Payload is a [`TrayCommand`] serialized as JSON.
pub const TRAY_EVENT_NAME: &str = "coldvox://tray-command";

/// Commands the tray can request. Mirrors the overlay button set.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrayCommand {
    Start,
    Stop,
    TogglePause,
    Clear,
    Quit,
}

impl TrayCommand {
    fn menu_id(&self) -> &'static str {
        match self {
            TrayCommand::Start => "tray-start",
            TrayCommand::Stop => "tray-stop",
            TrayCommand::TogglePause => "tray-toggle-pause",
            TrayCommand::Clear => "tray-clear",
            TrayCommand::Quit => "tray-quit",
        }
    }
}

/// Build the system tray icon + initial menu, then register it in app state.
/// Called once from `setup`. Mirrors Handy's `TrayIconBuilder::new()` pattern.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let icon = app.default_window_icon().cloned();
    let menu = menu_for(app, OverlayStatus::Idle)?;

    let mut builder = TrayIconBuilder::new()
        .tooltip("ColdVox — idle")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let cmd = match event.id.as_ref() {
                "tray-start" => Some(TrayCommand::Start),
                "tray-stop" => Some(TrayCommand::Stop),
                "tray-toggle-pause" => Some(TrayCommand::TogglePause),
                "tray-clear" => Some(TrayCommand::Clear),
                "tray-quit" => Some(TrayCommand::Quit),
                _ => None,
            };
            if let Some(cmd) = cmd {
                let _ = app.emit(TRAY_EVENT_NAME, cmd);
            }
        });

    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    let tray = builder.build(app)?;
    app.manage(tray);
    Ok(())
}

/// Rebuild the tray menu + tooltip to reflect a new pipeline status. Cheap to
/// call on every snapshot emit. No-ops when the tray was never built (e.g. a
/// platform with no system-tray service) so `emit_and_resize` stays safe even
/// though `build()` tolerates failure and continues setup.
pub fn refresh(app: &AppHandle, status: OverlayStatus) {
    let tooltip = tooltip_for(status);
    // `try_state` returns `None` when nothing was `app.manage()`d, which is
    // exactly the case when `build()` failed at setup. Using `state()` here
    // would panic on that path.
    let Some(tray) = app.try_state::<TrayIcon>() else {
        return;
    };
    let _ = tray.set_tooltip(Some(&tooltip));
    match menu_for(app, status) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(err) => warn!("failed to rebuild tray menu: {err}"),
    }
}

fn tooltip_for(status: OverlayStatus) -> String {
    match status {
        OverlayStatus::Idle => "ColdVox — idle".to_string(),
        OverlayStatus::Listening => "ColdVox — listening".to_string(),
        OverlayStatus::Processing => "ColdVox — processing".to_string(),
        OverlayStatus::Ready => "ColdVox — ready".to_string(),
        OverlayStatus::Error => "ColdVox — error".to_string(),
    }
}

fn menu_for(app: &AppHandle, status: OverlayStatus) -> tauri::Result<Menu> {
    let status_label = MenuItem::with_id(
        app,
        "tray-status",
        &tooltip_for(status),
        false,
        None::<&str>,
    )?;
    let sep = || PredefinedMenuItem::separator(app);

    // Disable Start while active; keep Stop enabled whenever the runtime may
    // still be alive — including Error, since an STT failure does not
    // automatically tear down the pipeline and the user needs a way to stop
    // capture via the tray.
    let (start_enabled, stop_enabled) = match status {
        OverlayStatus::Idle | OverlayStatus::Ready => (true, false),
        OverlayStatus::Listening | OverlayStatus::Processing | OverlayStatus::Error => (false, true),
    };

    let start = MenuItem::with_id(
        app,
        TrayCommand::Start.menu_id(),
        "Start pipeline",
        start_enabled,
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(
        app,
        TrayCommand::Stop.menu_id(),
        "Stop pipeline",
        stop_enabled,
        None::<&str>,
    )?;
    let pause = MenuItem::with_id(
        app,
        TrayCommand::TogglePause.menu_id(),
        "Pause / Resume",
        status == OverlayStatus::Listening,
        None::<&str>,
    )?;
    let clear = MenuItem::with_id(
        app,
        TrayCommand::Clear.menu_id(),
        "Clear transcript",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        TrayCommand::Quit.menu_id(),
        "Quit",
        true,
        None::<&str>,
    )?;

    Menu::with_items(
        app,
        &[
            &status_label,
            &sep(),
            &start,
            &stop,
            &pause,
            &clear,
            &sep(),
            &quit,
        ],
    )
}
