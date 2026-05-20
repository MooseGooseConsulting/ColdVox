mod state;
mod window;
mod contract;

use std::sync::Arc;

use contract::{OverlayEvent, OverlaySnapshot, OVERLAY_EVENT_NAME, OverlayStatus};
use state::OverlayModel;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use coldvox_app::runtime::{self as app_runtime, AppHandle as ColdVoxHandle, AppRuntimeOptions, ActivationMode};
use coldvox_app::stt::TranscriptionEvent;
use tokio::sync::Mutex as AsyncMutex;

#[derive(Default)]
struct OverlayRuntime {
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
    Ok(snapshot.clone())
}

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
    emit_and_resize(
        &app,
        &window,
        &snapshot,
        if expanded { "expanded" } else { "collapsed" },
    )
}

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

    let opts = AppRuntimeOptions {
        activation_mode: ActivationMode::AlwaysOnPushToTranscribe,
        stt_selection: None,
        enable_device_monitor: true,
        ..Default::default()
    };

    let mut coldvox_app = app_runtime::start(opts).await
        .map_err(|e| format!("Failed to start ColdVox runner: {}", e))?;

    let mut stt_rx = coldvox_app.stt_rx.take()
        .ok_or_else(|| "STT channel not available".to_string())?;

    let coldvox_app = Arc::new(coldvox_app);

    let model_clone = runtime.model.clone();
    let app_clone = app.clone();
    let window_clone = window.clone();

    // Spawn STT event listener
    tokio::spawn(async move {
        while let Some(event) = stt_rx.recv().await {
            let snapshot = {
                let mut model = model_clone.lock();
                match event {
                    TranscriptionEvent::Partial { text, .. } => model.update_partial(text),
                    TranscriptionEvent::Final { text, .. } => model.update_final(text),
                    TranscriptionEvent::Error { message, .. } => {
                        model.set_status(OverlayStatus::Error, message)
                    }
                }
            };
            let _ = emit_and_resize(&app_clone, &window_clone, &snapshot, "stt-update");
        }
    });

    *handle_guard = Some(coldvox_app);

    let snapshot = runtime.with_model(|model| {
        model.set_status(OverlayStatus::Listening, "Pipeline started (Always-On Mode)".to_string())
    });

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
        handle.shutdown().await;
        let snapshot = runtime.with_model(|model| {
            model.reset_to_idle("Pipeline stopped.".to_string())
        });
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

// Utility removed - replaced by real events

pub fn run() {
    tauri::Builder::default()
        .manage(OverlayRuntime::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let runtime = app.state::<OverlayRuntime>();
                let snapshot = runtime.snapshot();

                if let Err(error) = sync_window(&window, &snapshot) {
                    eprintln!("coldvox-gui window sync failed: {error}");
                }

                if let Err(error) = window.center() {
                    eprintln!("coldvox-gui window center failed: {error}");
                }
            }

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
}
