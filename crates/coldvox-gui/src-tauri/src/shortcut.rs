//! Global toggle shortcut for the ColdVox overlay.
//!
//! Ported in spirit from Handy (`cjpais/handy` → `src-tauri/src/shortcut/`):
//! a single global hotkey flips the pipeline between idle and active. The
//! default is `CmdOrCtrl+Shift+Space`, matching the brief in the Tauri-vs-Qt
//! comparison notes and behaving correctly on both Windows (Ctrl) and
//! macOS/Linux (`CmdOrCtrl` resolves per-platform).
//!
//! The plugin is initialized in `lib.rs` via `tauri_plugin_global_shortcut::init()`;
//! this module only parses + registers the accelerator (using the same
//! `on_shortcut` + `parse::<Shortcut>()` pattern Handy uses) and routes presses
//! through the same tray dispatch event. All real lifecycle logic stays in
//! `lib.rs`'s command bodies.

use crate::contract::OverlayStatus;
use crate::tray::{TrayCommand, TRAY_EVENT_NAME};
use crate::OverlayRuntime;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Default toggle accelerator. `CmdOrCtrl` resolves to `Cmd` on macOS and
/// `Ctrl` elsewhere, so one string covers Windows + Linux + macOS. Must match
/// the default in `settings::OverlaySettings::default().toggle_shortcut`.
pub const DEFAULT_TOGGLE_SHORTCUT: &str = "CmdOrCtrl+Shift+Space";

/// Parse and register the global toggle shortcut. Called once from `setup`.
pub fn register_toggle(app: &AppHandle) -> Result<(), String> {
    let shortcut = DEFAULT_TOGGLE_SHORTCUT
        .parse::<Shortcut>()
        .map_err(|e| format!("failed to parse toggle shortcut '{DEFAULT_TOGGLE_SHORTCUT}': {e}"))?;

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _scut, event| {
            if event.state == ShortcutState::Pressed {
                handle_pressed(app);
            }
        })
        .map_err(|e| format!("failed to register toggle shortcut: {e}"))?;

    Ok(())
}

/// Read the current pipeline status (a cheap synchronous `parking_lot` lock)
/// and emit the appropriate [`TrayCommand`] — Start when idle/ready, Stop when
/// active. Error status also routes to Start so a toggle recovers from a
/// failed pipeline by attempting a fresh start (the prior runtime is torn down
/// first if still present — see `start_pipeline`'s guard).
pub fn handle_pressed(app: &AppHandle) {
    let status = app.state::<OverlayRuntime>().current_status();
    let cmd = match status {
        OverlayStatus::Idle | OverlayStatus::Ready | OverlayStatus::Error => TrayCommand::Start,
        OverlayStatus::Listening | OverlayStatus::Processing => TrayCommand::Stop,
    };
    let _ = app.emit(TRAY_EVENT_NAME, cmd);
}
