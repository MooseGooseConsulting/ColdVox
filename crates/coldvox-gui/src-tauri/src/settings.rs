//! Persisted overlay settings, backed by `tauri-plugin-store`.
//!
//! Handy persists a large `AppSettings` struct via a JSON store; ColdVox's GUI
//! crate only needs a handful of overlay-relevant knobs, so this is a lean
//! port: toggle shortcut, overlay visibility/position, and the audio-feedback
//! flag. Everything else (model selection, paste method, etc.) lives in the
//! ColdVox app runtime config, not the GUI.

use log::warn;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "overlay";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OverlaySettings {
    /// Accelerator string for the global toggle shortcut. Must match
    /// [`crate::shortcut::DEFAULT_TOGGLE_SHORTCUT`] (`CmdOrCtrl+Shift+Space`)
    /// so the persisted default agrees with the actually-registered hotkey on
    /// every platform. Reserved for a follow-up that lets the settings UI
    /// re-register the shortcut; the first port always registers the compiled
    /// default.
    pub toggle_shortcut: String,
    /// Whether the overlay window is shown at all.
    pub overlay_enabled: bool,
    /// Where the overlay docks.
    pub overlay_position: OverlayPosition,
    /// Whether start/stop audio cues play (only effective under the
    /// `audio-feedback` cargo feature).
    pub audio_feedback: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    #[default]
    Top,
    Bottom,
    None,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            // Keep in sync with `shortcut::DEFAULT_TOGGLE_SHORTCUT`. Spelled
            // out here (rather than referenced) so `Default` stays a const-ish
            // fold with no cross-module dependency.
            toggle_shortcut: "CmdOrCtrl+Shift+Space".to_string(),
            overlay_enabled: true,
            overlay_position: OverlayPosition::Top,
            audio_feedback: false,
        }
    }
}

/// Load settings from the store, falling back to defaults on any error so the
/// GUI always boots with sane behavior even on a fresh install or a corrupt
/// store file.
pub fn load(app: &AppHandle) -> OverlaySettings {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(err) => {
            warn!("overlay settings store unavailable, using defaults: {err}");
            return OverlaySettings::default();
        }
    };

    match store.get(STORE_KEY) {
        Some(value) => serde_json::from_value::<OverlaySettings>(value).unwrap_or_else(|err| {
            warn!("overlay settings parse failed, using defaults: {err}");
            OverlaySettings::default()
        }),
        None => OverlaySettings::default(),
    }
}

/// Persist settings. Best-effort: logs on failure but never panics.
pub fn save(app: &AppHandle, settings: &OverlaySettings) {
    let Ok(store) = app.store(STORE_FILE) else {
        warn!("overlay settings store unavailable, cannot save");
        return;
    };
    match serde_json::to_value(settings) {
        Ok(value) => {
            store.set(STORE_KEY, value);
            // Honor the docstring's "logs on failure" contract: a disk-full or
            // permissions error here would otherwise vanish silently.
            if let Err(err) = store.save() {
                warn!("overlay settings save failed: {err}");
            }
        }
        Err(err) => warn!("overlay settings serialize failed: {err}"),
    }
}
