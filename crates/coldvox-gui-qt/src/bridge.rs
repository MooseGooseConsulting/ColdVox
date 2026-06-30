// CXX-Qt bridge between the Rust ColdVox pipeline and the QML overlay frontend.
//
// Adapted from PR #416's bridge, refactored to live in its own crate
// (`coldvox-gui-qt`) so the Tauri backend (`coldvox-gui`) and the Qt backend
// coexist as parallel GUIs over the same `coldvox_app::runtime` seam. The
// detailed per-plugin whisper config from #416 is replaced with
// `PluginSelectionConfig::default()` to match the Tauri backend's wiring; build
// `coldvox-app` with a real STT feature (parakeet/whisper) for transcription.
//
// Only compiled under the `qt-ui` feature (see `main.rs` gating).
//
// The `GuiBridge` qobject is registered as a QML element (`#[qml_element]`)
// under the `ColdVox` URI (see `build.rs`'s `qml_module` call), so QML can
// instantiate it directly: `GuiBridge { id: bridge }`.

/// Pipeline state exposed to QML as a Q_ENUM.
#[cxx_qt::qenum]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Idle,
    Activating,
    Active,
    Paused,
    Stopping,
    Error,
}

impl Default for AppState {
    fn default() -> Self {
        Self::Idle
    }
}

#[cxx_qt::bridge]
mod ffi {
    use cxx_qt_lib::QString;

    unsafe extern "RustQt" {
        #[qenum]
        type AppState = super::AppState;

        // Registered as a QML element so QML can instantiate `GuiBridge { id: bridge }`.
        // The build.rs `qml_module("ColdVox", 1, 0, ...)` call registers the URI.
        #[qml_element]
        #[qobject]
        #[qproperty(bool, expanded)]
        #[qproperty(AppState, state)]
        #[qproperty(String, last_error)]
        #[qproperty(String, partial_transcript)]
        #[qproperty(String, final_transcript)]
        type GuiBridge = super::GuiBridgeRust;

        // High-frequency partial update signal (QML debounces rendering).
        #[qsignal]
        fn transcript_partial(self: Pin<&mut Self>, text: QString);

        // Finalized utterance signal (replaces the partial).
        #[qsignal]
        fn transcript_final(self: Pin<&mut Self>, text: QString);

        /// Start the STT pipeline. Idle -> Activating -> Active.
        #[qinvokable]
        fn cmd_start(self: Pin<&mut Self>);

        /// Stop the pipeline. Active/Paused -> Stopping -> Idle.
        /// Performs a real `shutdown().await` on the runtime handle.
        #[qinvokable]
        fn cmd_stop(self: Pin<&mut Self>);

        /// Pause. Active -> Paused.
        #[qinvokable]
        fn cmd_pause(self: Pin<&mut Self>);

        /// Resume. Paused -> Active.
        #[qinvokable]
        fn cmd_resume(self: Pin<&mut Self>);

        /// Clear error state. Error -> Idle.
        #[qinvokable]
        fn cmd_clear_error(self: Pin<&mut Self>);

        /// Clear all transcript state and reset to Idle.
        #[qinvokable]
        fn cmd_clear(self: Pin<&mut Self>);
    }
}

use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use cxx_qt_lib::QString;

use coldvox_app::runtime::AppHandle as ColdVoxHandle;

/// Module-level slot for the live pipeline handle. The bridge is a singleton
/// (only one pipeline can run at a time), so a static is appropriate here and
/// avoids the need to thread an `Arc<Mutex<…>>` through CXX-Qt's qobject field
/// accessors (which don't easily expose non-QML types like `Arc<AppHandle>`).
/// `cmd_start` stores the handle here after a successful `runtime::start`;
/// `cmd_stop` takes it out and calls `shutdown().await`.
fn runtime_slot() -> &'static Mutex<Option<Arc<ColdVoxHandle>>> {
    static SLOT: OnceLock<Mutex<Option<Arc<ColdVoxHandle>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[derive(Default)]
pub struct GuiBridgeRust {
    expanded: bool,
    state: AppState,
    last_error: String,
    partial_transcript: String,
    final_transcript: String,
}

impl GuiBridge {
    /// Start the ColdVox pipeline on a dedicated Tokio runtime thread so
    /// blocking async I/O (audio capture, model loading) never stalls Qt.
    pub fn cmd_start(self: Pin<&mut Self>) {
        let current_state = *self.as_ref().state();
        if current_state != AppState::Idle {
            tracing::warn!("cmd_start called in state {:?}, ignoring", current_state);
            return;
        }

        self.set_state(AppState::Activating);

        let qt_thread = Self::qt_thread(self);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build Tokio runtime for ColdVox pipeline");
            let _guard = rt.enter();

            rt.block_on(async {
                use coldvox_app::runtime::{AppRuntimeOptions, ActivationMode};

                // Match the Tauri backend: leave stt_selection None so the
                // runtime boots audio + VAD without activating STT. The
                // `http-remote` backend is compiled in via Cargo.toml; turn
                // it on here in a follow-up once the Qt settings UI exposes
                // endpoint configuration.
                let opts = AppRuntimeOptions {
                    activation_mode: ActivationMode::AlwaysOnPushToTranscribe,
                    stt_selection: None,
                    enable_device_monitor: true,
                    ..Default::default()
                };

                match coldvox_app::runtime::start(opts).await {
                    Ok(mut app) => {
                        let stt_rx = app.stt_rx.take();
                        let shared = Arc::new(app);

                        // Store the handle so cmd_stop can shut it down.
                        if let Ok(mut slot) = runtime_slot().lock() {
                            *slot = Some(shared.clone());
                        }

                        if let Some(mut rx) = stt_rx {
                            // Clone qt_thread for the STT listener task. The
                            // original `qt_thread` stays usable for the
                            // post-start `set_state(Active)` queue below.
                            // Without this clone, the `async move` capture
                            // would move `qt_thread` and the later queue call
                            // would be a use-after-move (compile error).
                            let qt_thread_for_stt = qt_thread.clone();
                            tokio::spawn(async move {
                                while let Some(event) = rx.recv().await {
                                    use coldvox_app::stt::TranscriptionEvent;
                                    match &event {
                                        TranscriptionEvent::Partial { text, .. } => {
                                            let owned = text.to_string();
                                            let q = QString::from(&owned);
                                            qt_thread_for_stt.queue(move |mut b| {
                                                b.as_mut().set_partial_transcript(q.clone());
                                                b.as_mut().transcript_partial(q);
                                            });
                                        }
                                        TranscriptionEvent::Final { text, .. } => {
                                            let owned = text.to_string();
                                            let q = QString::from(&owned);
                                            qt_thread_for_stt.queue(move |mut b| {
                                                let existing =
                                                    b.as_ref().final_transcript().to_string();
                                                let merged = if existing.is_empty() {
                                                    owned.clone()
                                                } else {
                                                    format!("{existing}\n{owned}")
                                                };
                                                b.as_mut()
                                                    .set_final_transcript(QString::from(&merged));
                                                b.as_mut()
                                                    .set_partial_transcript(QString::default());
                                                b.as_mut().transcript_final(q);
                                            });
                                        }
                                        TranscriptionEvent::Error { message, .. } => {
                                            let msg = message.to_string();
                                            qt_thread_for_stt.queue(move |mut b| {
                                                b.as_mut().set_last_error(QString::from(&msg));
                                                b.as_mut().set_state(AppState::Error);
                                            });
                                        }
                                    }
                                }
                            });
                        }

                        tracing::info!("ColdVox pipeline started (Qt backend)");
                        // Queue the Active transition from the spawned thread.
                        // Do NOT also set it optimistically on the Qt thread
                        // (the prior version did both, which raced: a fast
                        // failure would flicker Idle → Activating → Active → Error).
                        let _ = qt_thread.queue(|mut b| {
                            b.as_mut().set_state(AppState::Active);
                        });

                        // Keep the runtime alive until cmd_stop or process exit.
                        let _keep_alive = shared;
                        std::future::pending::<()>().await;
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        tracing::error!("Failed to start ColdVox pipeline: {msg}");
                        let _ = qt_thread.queue(move |mut b| {
                            b.as_mut().set_last_error(QString::from(&msg));
                            b.as_mut().set_state(AppState::Error);
                        });
                    }
                }
            });
        });
    }

    /// Stop the pipeline. Takes the live `Arc<ColdVoxHandle>` out of the
    /// runtime slot and calls `shutdown().await` on a dedicated thread (the
    /// qinvokable is sync, but `shutdown` is async). Matches the Tauri
    /// backend's `stop_pipeline_inner` semantics.
    pub fn cmd_stop(self: Pin<&mut Self>) {
        let current = *self.as_ref().state();
        if !matches!(current, AppState::Active | AppState::Paused) {
            tracing::warn!("cmd_stop called in state {:?}, ignoring", current);
            return;
        }
        self.set_state(AppState::Stopping);

        let handle = runtime_slot().lock().ok().and_then(|mut g| g.take());
        if let Some(handle) = handle {
            // `shutdown` is async and takes `self: Arc<Self>`, so spawn a
            // thread with a one-shot runtime. This mirrors the cmd_start
            // pattern and avoids blocking the Qt event loop.
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build runtime for ColdVox shutdown");
                rt.block_on(async {
                    handle.shutdown().await;
                });
            });
        } else {
            tracing::warn!(
                "cmd_stop: no live runtime handle in slot; transitioning to Idle anyway"
            );
        }

        self.set_state(AppState::Idle);
    }

    pub fn cmd_pause(self: Pin<&mut Self>) {
        if *self.as_ref().state() == AppState::Active {
            self.set_state(AppState::Paused);
        } else {
            tracing::warn!("cmd_pause called in non-Active state");
        }
    }

    pub fn cmd_resume(self: Pin<&mut Self>) {
        if *self.as_ref().state() == AppState::Paused {
            self.set_state(AppState::Active);
        } else {
            tracing::warn!("cmd_resume called in non-Paused state");
        }
    }

    pub fn cmd_clear_error(mut self: Pin<&mut Self>) {
        if *self.as_ref().state() == AppState::Error {
            self.as_mut().set_state(AppState::Idle);
            self.as_mut().set_last_error(QString::from(""));
        }
    }

    pub fn cmd_clear(mut self: Pin<&mut Self>) {
        self.as_mut().set_partial_transcript(QString::default());
        self.as_mut().set_final_transcript(QString::default());
        if matches!(*self.as_ref().state(), AppState::Active | AppState::Paused) {
            self.as_mut().set_state(AppState::Stopping);
        }
        self.as_mut().set_state(AppState::Idle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cxx_qt::CxxQtThread;

    fn create_bridge() -> Pin<Box<GuiBridge>> {
        GuiBridge::new()
    }

    #[test]
    fn initial_state_is_idle() {
        let b = create_bridge();
        assert_eq!(*b.as_ref().state(), AppState::Idle);
        assert_eq!(*b.as_ref().expanded(), false);
    }

    #[test]
    fn pause_resume_roundtrip_offline() {
        let mut b = create_bridge();
        b.as_mut().set_state(AppState::Active);
        b.as_mut().cmd_pause();
        assert_eq!(*b.as_ref().state(), AppState::Paused);
        b.as_mut().cmd_resume();
        assert_eq!(*b.as_ref().state(), AppState::Active);
    }

    #[test]
    fn clear_resets_transcript() {
        let mut b = create_bridge();
        b.as_mut().set_state(AppState::Active);
        b.as_mut().set_partial_transcript(QString::from("hello"));
        b.as_mut().cmd_clear();
        assert_eq!(*b.as_ref().state(), AppState::Idle);
        assert!(b.as_ref().partial_transcript().is_empty());
    }

    #[test]
    fn clear_error_only_from_error() {
        let mut b = create_bridge();
        b.as_mut().set_state(AppState::Error);
        b.as_mut().set_last_error(QString::from("boom"));
        b.as_mut().cmd_clear_error();
        assert_eq!(*b.as_ref().state(), AppState::Idle);
        assert_eq!(*b.as_ref().last_error(), "");
    }

    // Reference the trait to keep the import meaningful for the offline tests.
    #[test]
    fn cxx_qt_thread_trait_in_scope() {
        fn _assert<T: CxxQtThread>() {}
    }
}
