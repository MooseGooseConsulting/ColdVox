//! Tauri host shell for the ColdVox overlay.
//!
//! Built on the tauri-base pipeline wiring (PR #434) and layered with
//! Handy-inspired desktop affordances: a system tray, a global toggle
//! shortcut, optional start/stop audio cues, persisted overlay settings, and a
//! live mic-level meter forwarded to the React overlay.
//!
//! Architecture:
//! - [`OverlayRuntime`] holds the shared [`OverlayModel`] and the optional
//!   live ColdVox `AppHandle` (wrapped in `Arc` so `shutdown(self: Arc<Self>)`
//!   is callable without re-wrapping).
//! - `#[tauri::command]` functions are thin wrappers so the tray/shortcut
//!   dispatch path can reuse the exact same lifecycle logic (single source of
//!   truth).
//! - The STT event listener forwards `TranscriptionEvent::{Partial,Final,Error}`
//!   into the model, re-emits the snapshot to the webview, and refreshes the
//!   tray icon/menu.

mod audio_feedback;
mod contract;
mod settings;
mod shortcut;
mod state;
mod tray;
mod window;

use std::sync::Arc;

use coldvox_app::runtime::{
    self as app_runtime, ActivationMode, AppHandle as ColdVoxHandle, AppRuntimeOptions,
};
use coldvox_app::stt::TranscriptionEvent;
use coldvox_audio::ResamplerQuality;
use contract::{OverlayEvent, OverlaySnapshot, OverlayStatus, OVERLAY_EVENT_NAME};
use state::OverlayModel;
use tauri::{AppHandle, Emitter, Listener, Manager, State, WebviewWindow};
use tokio::sync::Mutex as AsyncMutex;

use crate::tray::TrayCommand;

/// Shared GUI state. Cheap to clone the `Arc`s out for async work.
#[derive(Default)]
pub(crate) struct OverlayRuntime {
    model: Arc<parking_lot::Mutex<OverlayModel>>,
    app_handle: Arc<AsyncMutex<Option<Arc<ColdVoxHandle>>>>,
}

impl OverlayRuntime {
    fn snapshot(&self) -> OverlaySnapshot {
        self.with_model(|model| model.snapshot())
    }

    fn with_model<R>(&self, update: impl FnOnce(&mut OverlayModel) -> R) -> R {
        let mut model = self.model.lock();
        update(&mut model)
    }

    /// Current pipeline status — used by the global shortcut to decide whether
    /// a toggle press should start or stop. Synchronous lock, safe off the
    /// async runtime.
    pub(crate) fn current_status(&self) -> OverlayStatus {
        self.with_model(|model| model.snapshot().status)
    }
}

type CommandResult = Result<OverlaySnapshot, String>;

fn emit_snapshot(app: &AppHandle, snapshot: &OverlaySnapshot, reason: &str) -> Result<(), String> {
    app.emit(
        OVERLAY_EVENT_NAME,
        OverlayEvent {
            reason: reason.to_string(),
            snapshot: snapshot.clone(),
        },
    )
    .map_err(|error| error.to_string())
}

fn sync_window(window: &WebviewWindow, snapshot: &OverlaySnapshot) -> Result<(), String> {
    window::sync_window(window, snapshot).map_err(|error| error.to_string())
}

fn emit_and_resize(
    app: &AppHandle,
    window: &WebviewWindow,
    snapshot: &OverlaySnapshot,
    reason: &str,
) -> CommandResult {
    sync_window(window, snapshot)?;
    emit_snapshot(app, snapshot, reason)?;
    tray::refresh(app, snapshot.status);
    Ok(snapshot.clone())
}

/// Root-mean-square level of an i16 audio frame, normalized to `0.0..=1.0`.
/// Used to drive the overlay's live mic-level meter.
fn rms_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&s| {
            let f = s as f64 / i16::MAX as f64;
            f * f
        })
        .sum();
    ((sum_sq / samples.len() as f64).sqrt() as f32).clamp(0.0, 1.0)
}

// ── Commands: read/state ───────────────────────────────────────────────────

#[tauri::command]
fn get_overlay_snapshot(runtime: State<'_, OverlayRuntime>) -> OverlaySnapshot {
    runtime.snapshot()
}

#[tauri::command]
fn set_overlay_expanded(
    expanded: bool,
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.set_expanded(expanded));
    emit_and_resize(&app, &window, &snapshot, if expanded { "expanded" } else { "collapsed" })
}

// ── Commands: lifecycle (async) ────────────────────────────────────────────

#[tauri::command]
async fn start_pipeline(
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let mut handle_guard = runtime.app_handle.lock().await;
    if handle_guard.is_some() {
        return Err("Pipeline already running".to_string());
    }

    // tauri-base deliberately leaves `stt_selection: None` so the runtime
    // boots audio + VAD without activating STT. The `http-remote` backend is
    // compiled in (see Cargo.toml) and can be turned on here in a follow-up
    // once the GUI exposes model/endpoint configuration.
    let opts = AppRuntimeOptions {
        activation_mode: ActivationMode::AlwaysOnPushToTranscribe,
        resampler_quality: ResamplerQuality::Balanced,
        stt_selection: None,
        enable_device_monitor: true,
        ..Default::default()
    };

    let mut coldvox_app = app_runtime::start(opts)
        .await
        .map_err(|e| format!("Failed to start ColdVox runner: {}", e))?;

    let mut stt_rx = coldvox_app
        .stt_rx
        .take()
        .ok_or_else(|| "STT channel not available".to_string())?;

    // Subscribe to raw audio frames so the overlay can render a live mic-level
    // meter (Handy-style). The receiver must be created before `coldvox_app`
    // is moved into the guard.
    let mut audio_rx = coldvox_app.subscribe_audio();
    let coldvox_app = Arc::new(coldvox_app);

    let model_clone = runtime.model.clone();
    let app_clone = app.clone();
    let window_clone = window.clone();

    // Spawn STT event listener.
    tokio::spawn(async move {
        while let Some(event) = stt_rx.recv().await {
            let snapshot = {
                let mut model = model_clone.lock();
                match event {
                    TranscriptionEvent::Partial { text, .. } => model.update_partial(text),
                    TranscriptionEvent::Final { text, .. } => model.update_final(text),
                    // `apply_error` sets BOTH `status_detail` and
                    // `error_message` so the React overlay's error badge
                    // (which reads `errorMessage`) actually surfaces the STT
                    // failure text. `set_status` alone only updates
                    // `status_detail`, leaving the badge blank.
                    TranscriptionEvent::Error { message, .. } => model.apply_error(message),
                }
            };
            let _ = emit_and_resize(&app_clone, &window_clone, &snapshot, "stt-update");
        }
    });

    // Mic-level meter stream: compute RMS per frame and emit to the overlay.
    // The React side throttles rendering via requestAnimationFrame, so emitting
    // per-frame here is acceptable (matches Handy's per-callback emission).
    // A `Lagged` receiver (slow consumer) is recoverable: skip the missed
    // frames and keep going. Only `Closed` (sender dropped) ends the stream —
    // treating `Lagged` as terminal would kill the meter for the whole session
    // after one slow moment.
    let app_for_audio = app.clone();
    tokio::spawn(async move {
        loop {
            match audio_rx.recv().await {
                Ok(frame) => {
                    let level = rms_level(&frame.samples);
                    let _ = app_for_audio.emit_to("main", "mic-level", level);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("mic-level receiver lagged by {n} frames; resuming");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    *handle_guard = Some(coldvox_app);

    let snapshot = runtime.with_model(|model| {
        model.set_status(
            OverlayStatus::Listening,
            "Pipeline started (Always-On Mode)".to_string(),
        )
    });

    audio_feedback::play(&app, audio_feedback::Cue::Start);
    emit_and_resize(&app, &window, &snapshot, "pipeline-started")
}

#[tauri::command]
async fn stop_pipeline(
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let mut handle_guard = runtime.app_handle.lock().await;
    if let Some(handle) = handle_guard.take() {
        // `shutdown` takes `self: Arc<Self>`, and `handle` is already
        // `Arc<ColdVoxHandle>`, so call directly — no re-wrap needed.
        handle.shutdown().await;
        let snapshot =
            runtime.with_model(|model| model.reset_to_idle("Pipeline stopped.".to_string()));
        audio_feedback::play(&app, audio_feedback::Cue::Stop);
        emit_and_resize(&app, &window, &snapshot, "pipeline-stopped")
    } else {
        Err("Pipeline not running".to_string())
    }
}

#[tauri::command]
fn toggle_pause_state(
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.toggle_pause());
    emit_and_resize(&app, &window, &snapshot, "pause-toggled")
}

#[tauri::command]
fn clear_overlay_transcript(
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.clear());
    emit_and_resize(&app, &window, &snapshot, "transcript-cleared")
}

#[tauri::command]
fn open_settings_placeholder(
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.open_settings_placeholder());
    emit_and_resize(&app, &window, &snapshot, "settings-placeholder")
}

// ── Commands: external STT-driver seam ─────────────────────────────────────
// These let an out-of-band STT driver push transcripts/state into the overlay
// without going through the in-process ColdVox runtime. The in-process runtime
// (start_pipeline) is the primary path; these commands exist for the contract
// the React bridge already declares on tauri-base.

#[tauri::command]
fn update_partial_transcript(
    text: String,
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.apply_partial_transcript(&text, None));
    emit_and_resize(&app, &window, &snapshot, "partial-transcript")
}

#[tauri::command]
fn update_final_transcript(
    text: String,
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.apply_final_transcript(&text, None));
    emit_and_resize(&app, &window, &snapshot, "final-transcript")
}

#[tauri::command]
fn set_overlay_processing(
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.apply_processing_state(None));
    emit_and_resize(&app, &window, &snapshot, "processing")
}

#[tauri::command]
fn set_overlay_listening(
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.apply_listening_state(None));
    emit_and_resize(&app, &window, &snapshot, "listening")
}

#[tauri::command]
fn stop_overlay_capture(
    runtime: State<'_, OverlayRuntime>,
    window: WebviewWindow,
    app: AppHandle,
) -> CommandResult {
    let snapshot = runtime.with_model(|model| model.stop_capture());
    emit_and_resize(&app, &window, &snapshot, "capture-stopped")
}

// ── Tray/shortcut dispatch ─────────────────────────────────────────────────
// Tray menu items and the global shortcut both emit `TrayCommand`s. This
// listener fans them out to the same lifecycle functions the commands use.

fn dispatch_tray_command(app: &AppHandle, cmd: TrayCommand) {
    match cmd {
        TrayCommand::Start => spawn_lifecycle(app, Lifecycle::Start),
        TrayCommand::Stop => spawn_lifecycle(app, Lifecycle::Stop),
        TrayCommand::TogglePause => {
            let runtime = app.state::<OverlayRuntime>();
            let snapshot = runtime.with_model(|m| m.toggle_pause());
            if let Some(window) = app.get_webview_window("main") {
                let _ = emit_and_resize(app, &window, &snapshot, "pause-toggled");
            }
        }
        TrayCommand::Clear => {
            let runtime = app.state::<OverlayRuntime>();
            let snapshot = runtime.with_model(|m| m.clear());
            if let Some(window) = app.get_webview_window("main") {
                let _ = emit_and_resize(app, &window, &snapshot, "transcript-cleared");
            }
        }
        TrayCommand::Quit => {
            app.exit(0);
        }
    }
}

#[derive(Clone, Copy)]
enum Lifecycle {
    Start,
    Stop,
}

fn spawn_lifecycle(app: &AppHandle, which: Lifecycle) {
    let runtime = app.state::<OverlayRuntime>();
    let model = runtime.model.clone();
    let slot = runtime.app_handle.clone();
    let app = app.clone();
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("main window unavailable; ignoring lifecycle command");
        return;
    };
    tauri::async_runtime::spawn(async move {
        let result = match which {
            Lifecycle::Start => start_pipeline_async(app.clone(), model, slot, window).await,
            Lifecycle::Stop => stop_pipeline_async(app.clone(), model, slot, window).await,
        };
        if let Err(err) = result {
            log::error!("tray/shortcut lifecycle command failed: {err}");
        }
    });
}

// Shared inner lifecycle bodies for the tray/shortcut dispatch path. These
// mirror the `#[tauri::command]` functions but take explicit `Arc` clones
// instead of `State<'_>` (which can't cross the `tauri::async_runtime::spawn`
// boundary).
async fn start_pipeline_async(
    app: AppHandle,
    model: Arc<parking_lot::Mutex<OverlayModel>>,
    slot: Arc<AsyncMutex<Option<Arc<ColdVoxHandle>>>>,
    window: WebviewWindow,
) -> CommandResult {
    let runtime = OverlayRuntime { model, app_handle: slot };
    // Reuse the command body by reconstructing the minimal runtime view.
    // We can't call the `#[tauri::command]` directly (it needs `State`), so
    // inline the same logic. Keeping the two in sync is acceptable for now;
    // a future refactor could extract a pure `start_pipeline_inner`.
    let mut handle_guard = runtime.app_handle.lock().await;
    if handle_guard.is_some() {
        return Err("Pipeline already running".to_string());
    }
    let opts = AppRuntimeOptions {
        activation_mode: ActivationMode::AlwaysOnPushToTranscribe,
        resampler_quality: ResamplerQuality::Balanced,
        stt_selection: None,
        enable_device_monitor: true,
        ..Default::default()
    };
    let mut coldvox_app = app_runtime::start(opts)
        .await
        .map_err(|e| format!("Failed to start ColdVox runner: {}", e))?;
    let mut stt_rx = coldvox_app
        .stt_rx
        .take()
        .ok_or_else(|| "STT channel not available".to_string())?;
    let mut audio_rx = coldvox_app.subscribe_audio();
    let coldvox_app = Arc::new(coldvox_app);

    let model_clone = runtime.model.clone();
    let app_clone = app.clone();
    let window_clone = window.clone();
    tokio::spawn(async move {
        while let Some(event) = stt_rx.recv().await {
            let snapshot = {
                let mut m = model_clone.lock();
                match event {
                    TranscriptionEvent::Partial { text, .. } => m.update_partial(text),
                    TranscriptionEvent::Final { text, .. } => m.update_final(text),
                    TranscriptionEvent::Error { message, .. } => m.apply_error(message),
                }
            };
            let _ = emit_and_resize(&app_clone, &window_clone, &snapshot, "stt-update");
        }
    });
    let app_for_audio = app.clone();
    tokio::spawn(async move {
        loop {
            match audio_rx.recv().await {
                Ok(frame) => {
                    let level = rms_level(&frame.samples);
                    let _ = app_for_audio.emit_to("main", "mic-level", level);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("mic-level receiver lagged by {n} frames; resuming");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    *handle_guard = Some(coldvox_app);
    let snapshot = runtime.with_model(|m| {
        m.set_status(
            OverlayStatus::Listening,
            "Pipeline started (Always-On Mode)".to_string(),
        )
    });
    audio_feedback::play(&app, audio_feedback::Cue::Start);
    emit_and_resize(&app, &window, &snapshot, "pipeline-started")
}

async fn stop_pipeline_async(
    app: AppHandle,
    model: Arc<parking_lot::Mutex<OverlayModel>>,
    slot: Arc<AsyncMutex<Option<Arc<ColdVoxHandle>>>>,
    window: WebviewWindow,
) -> CommandResult {
    let runtime = OverlayRuntime { model, app_handle: slot };
    let mut handle_guard = runtime.app_handle.lock().await;
    if let Some(handle) = handle_guard.take() {
        handle.shutdown().await;
        let snapshot = runtime.with_model(|m| m.reset_to_idle("Pipeline stopped.".to_string()));
        audio_feedback::play(&app, audio_feedback::Cue::Stop);
        emit_and_resize(&app, &window, &snapshot, "pipeline-stopped")
    } else {
        Err("Pipeline not running".to_string())
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .manage(OverlayRuntime::default())
        .setup(|app| {
            // Sync the overlay window to the initial (idle, collapsed) snapshot.
            if let Some(window) = app.get_webview_window("main") {
                let runtime = app.state::<OverlayRuntime>();
                let snapshot = runtime.snapshot();
                if let Err(error) = sync_window(&window, &snapshot) {
                    eprintln!("coldvox-gui window sync failed: {error}");
                }
                let _ = window.center();
            }

            // System tray (Handy port). Tolerates failure so a headless /
            // tray-less platform still boots the overlay.
            if let Err(err) = tray::build(app.handle()) {
                eprintln!("coldvox-gui tray build failed: {err}");
            }

            // Global toggle shortcut (Handy port).
            if let Err(err) = shortcut::register_toggle(app.handle()) {
                eprintln!("coldvox-gui shortcut register failed: {err}");
            }

            // Load persisted settings so the overlay honors the user's last
            // position/visibility choice on boot. (Full re-application of
            // overlay_position is a follow-up; the load is wired now.)
            let _settings = settings::load(app.handle());

            // Dispatch tray + shortcut commands through a single listener.
            let app_handle = app.handle().clone();
            app.handle().listen(tray::TRAY_EVENT_NAME, move |event| {
                let payload = event.payload();
                let Ok(cmd) = serde_json::from_str::<TrayCommand>(payload.unwrap_or("")) else {
                    log::warn!("ignoring malformed tray command payload");
                    return;
                };
                dispatch_tray_command(&app_handle, cmd);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_overlay_snapshot,
            set_overlay_expanded,
            start_pipeline,
            stop_pipeline,
            toggle_pause_state,
            clear_overlay_transcript,
            open_settings_placeholder,
            update_partial_transcript,
            update_final_transcript,
            set_overlay_processing,
            set_overlay_listening,
            stop_overlay_capture,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::contract::{OverlayEvent, OverlaySnapshot, OverlayStatus};

    #[test]
    fn overlay_event_serializes_camel_case_contract_fields() {
        let payload = OverlayEvent {
            reason: "contract-check".to_string(),
            snapshot: OverlaySnapshot {
                expanded: true,
                status: OverlayStatus::Ready,
                paused: false,
                partial_transcript: String::new(),
                final_transcript: "final transcript".to_string(),
                status_detail: "ready".to_string(),
                error_message: None,
            },
        };

        let json = serde_json::to_string(&payload).expect("serialize overlay event");

        assert!(json.contains("partialTranscript"));
        assert!(json.contains("finalTranscript"));
        assert!(json.contains("statusDetail"));
        assert!(json.contains("ready"));
    }

    #[test]
    fn tray_command_round_trips_serde() {
        let cmd = super::TrayCommand::Start;
        let json = serde_json::to_string(&cmd).expect("serialize tray command");
        let back: super::TrayCommand =
            serde_json::from_str(&json).expect("deserialize tray command");
        assert_eq!(cmd, back);
    }
}
