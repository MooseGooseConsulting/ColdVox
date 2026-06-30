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

        /// Stop the pipeline. Active/Paused -> Idle.
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
                use coldvox_stt::plugin::PluginSelectionConfig;

                let opts = AppRuntimeOptions {
                    activation_mode: ActivationMode::AlwaysOnPushToTranscribe,
                    stt_selection: Some(PluginSelectionConfig::default()),
                    enable_device_monitor: true,
                    ..Default::default()
                };

                match coldvox_app::runtime::start(opts).await {
                    Ok(mut app) => {
                        let stt_rx = app.stt_rx.take();
                        let shared = std::sync::Arc::new(app);

                        if let Some(mut rx) = stt_rx {
                            tokio::spawn(async move {
                                while let Some(event) = rx.recv().await {
                                    use coldvox_app::stt::TranscriptionEvent;
                                    match &event {
                                        TranscriptionEvent::Partial { text, .. } => {
                                            let owned = text.to_string();
                                            let q = QString::from(&owned);
                                            qt_thread.queue(move |mut b| {
                                                b.as_mut().set_partial_transcript(q.clone());
                                                b.as_mut().transcript_partial(q);
                                            });
                                        }
                                        TranscriptionEvent::Final { text, .. } => {
                                            let owned = text.to_string();
                                            let q = QString::from(&owned);
                                            qt_thread.queue(move |mut b| {
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
                                            qt_thread.queue(move |mut b| {
                                                b.as_mut().set_last_error(QString::from(&msg));
                                                b.as_mut().set_state(AppState::Error);
                                            });
                                        }
                                    }
                                }
                            });
                        }

                        tracing::info!("ColdVox pipeline started (Qt backend)");
                        let _ = qt_thread.queue(|mut b| {
                            b.as_mut().set_state(AppState::Active);
                        });

                        // Keep the runtime alive until the process exits. A
                        // future cmd_stop that holds the Arc can shut it down
                        // gracefully (tracked as a follow-up — see PR notes).
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

        // Optimistically flip to Active; errors surface via the STT listener.
        self.set_state(AppState::Active);
    }

    /// Stop the pipeline. NOTE: full graceful shutdown requires holding the
    /// `AppHandle` Arc across the bridge (tracked follow-up); this transitions
    /// state so the UI reflects the intent. The Tauri backend performs a real
    /// `shutdown().await` today.
    pub fn cmd_stop(self: Pin<&mut Self>) {
        let current = *self.as_ref().state();
        if !matches!(current, AppState::Active | AppState::Paused) {
            tracing::warn!("cmd_stop called in state {:?}, ignoring", current);
            return;
        }
        self.set_state(AppState::Stopping);
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
