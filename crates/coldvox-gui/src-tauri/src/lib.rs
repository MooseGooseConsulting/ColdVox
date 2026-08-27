mod contract;
mod demo;
mod state;
mod window;

use std::sync::Mutex;

use contract::{OverlayEvent, OverlaySnapshot, OVERLAY_EVENT_NAME};
use demo::demo_script;
use state::OverlayModel;
use tauri::{AppHandle, Emitter, Manager, State};
use window::sync_window;

type OverlayState = Mutex<OverlayModel>;

/// Emit the snapshot as a `coldvox://overlay` event so the React frontend can
/// react without polling.
fn emit_snapshot(app: &AppHandle, reason: &str, snapshot: &OverlaySnapshot) {
    let event = OverlayEvent {
        reason: reason.to_string(),
        snapshot: snapshot.clone(),
    };
    let _ = app.emit(OVERLAY_EVENT_NAME, event);
}

// ---------------------------------------------------------------------------
// Tauri commands — each returns the new snapshot so the frontend can update
// synchronously without waiting for the event.
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_overlay_snapshot(state: State<'_, OverlayState>) -> OverlaySnapshot {
    state.lock().unwrap().snapshot()
}

#[tauri::command]
fn set_overlay_expanded(
    expanded: bool,
    app: AppHandle,
    state: State<'_, OverlayState>,
) -> tauri::Result<OverlaySnapshot> {
    let mut model = state.lock().unwrap();
    let snapshot = model.set_expanded(expanded);
    if let Some(window) = app.get_webview_window("main") {
        sync_window(&window, &snapshot)?;
    }
    emit_snapshot(&app, "set_expanded", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn start_pipeline(
    app: AppHandle,
    state: State<'_, OverlayState>,
) -> tauri::Result<OverlaySnapshot> {
    let (token, start_snapshot) = {
        let mut model = state.lock().unwrap();
        model.start_demo()
    };
    if let Some(window) = app.get_webview_window("main") {
        sync_window(&window, &start_snapshot)?;
    }
    emit_snapshot(&app, "start_pipeline", &start_snapshot);

    // Drive the demo steps asynchronously.
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        for step in demo_script() {
            // Exit early if a stop/clear has invalidated this demo session.
            {
                let model = app_handle.state::<OverlayState>().inner().lock().unwrap();
                if model.current_demo_token() != token {
                    return;
                }
            }

            if let demo::DemoStep::Wait(ms) = &step {
                tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
                continue;
            }

            let snapshot = {
                let mut model = app_handle.state::<OverlayState>().inner().lock().unwrap();
                if model.current_demo_token() != token {
                    return;
                }
                model.apply_demo_step(&step)
            };
            emit_snapshot(&app_handle, "demo_step", &snapshot);
        }
    });

    Ok(start_snapshot)
}

#[tauri::command]
fn toggle_pause_state(app: AppHandle, state: State<'_, OverlayState>) -> OverlaySnapshot {
    let snapshot = state.lock().unwrap().toggle_pause();
    emit_snapshot(&app, "toggle_pause", &snapshot);
    snapshot
}

#[tauri::command]
fn stop_pipeline(app: AppHandle, state: State<'_, OverlayState>) -> tauri::Result<OverlaySnapshot> {
    let snapshot = state.lock().unwrap().stop();
    if let Some(window) = app.get_webview_window("main") {
        sync_window(&window, &snapshot)?;
    }
    emit_snapshot(&app, "stop_pipeline", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn clear_overlay_transcript(
    app: AppHandle,
    state: State<'_, OverlayState>,
) -> tauri::Result<OverlaySnapshot> {
    let snapshot = state.lock().unwrap().clear();
    if let Some(window) = app.get_webview_window("main") {
        sync_window(&window, &snapshot)?;
    }
    emit_snapshot(&app, "clear", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn open_settings_placeholder(
    app: AppHandle,
    state: State<'_, OverlayState>,
) -> tauri::Result<OverlaySnapshot> {
    let snapshot = state.lock().unwrap().open_settings_placeholder();
    if let Some(window) = app.get_webview_window("main") {
        sync_window(&window, &snapshot)?;
    }
    emit_snapshot(&app, "open_settings", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn update_partial_transcript(
    text: String,
    app: AppHandle,
    state: State<'_, OverlayState>,
) -> OverlaySnapshot {
    let snapshot = state.lock().unwrap().apply_partial_transcript(&text, None);
    emit_snapshot(&app, "partial_transcript", &snapshot);
    snapshot
}

#[tauri::command]
fn update_final_transcript(
    text: String,
    app: AppHandle,
    state: State<'_, OverlayState>,
) -> tauri::Result<OverlaySnapshot> {
    let snapshot = state.lock().unwrap().apply_final_transcript(&text, None);
    if let Some(window) = app.get_webview_window("main") {
        sync_window(&window, &snapshot)?;
    }
    emit_snapshot(&app, "final_transcript", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn set_overlay_processing(app: AppHandle, state: State<'_, OverlayState>) -> OverlaySnapshot {
    let snapshot = state.lock().unwrap().apply_processing_state(None);
    emit_snapshot(&app, "processing", &snapshot);
    snapshot
}

#[tauri::command]
fn set_overlay_listening(
    app: AppHandle,
    state: State<'_, OverlayState>,
) -> tauri::Result<OverlaySnapshot> {
    let snapshot = state.lock().unwrap().apply_listening_state(None);
    if let Some(window) = app.get_webview_window("main") {
        sync_window(&window, &snapshot)?;
    }
    emit_snapshot(&app, "listening", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn stop_overlay_capture(
    app: AppHandle,
    state: State<'_, OverlayState>,
) -> tauri::Result<OverlaySnapshot> {
    let snapshot = state.lock().unwrap().stop_capture();
    if let Some(window) = app.get_webview_window("main") {
        sync_window(&window, &snapshot)?;
    }
    emit_snapshot(&app, "stop_capture", &snapshot);
    Ok(snapshot)
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

pub fn run() {
    tauri::Builder::default()
        .manage(OverlayState::default())
        .invoke_handler(tauri::generate_handler![
            get_overlay_snapshot,
            set_overlay_expanded,
            start_pipeline,
            toggle_pause_state,
            stop_pipeline,
            clear_overlay_transcript,
            open_settings_placeholder,
            update_partial_transcript,
            update_final_transcript,
            set_overlay_processing,
            set_overlay_listening,
            stop_overlay_capture,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ColdVox GUI");
}
