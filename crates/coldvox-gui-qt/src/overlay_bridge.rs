/// Tauri-free overlay state machine that mirrors the contract from the Tauri
/// backend.  Exposed to QML via the cxx-qt bridge as `OverlayBridge`.
///
/// The demo driver is intentionally driven by a QML `Timer` (see
/// `qml/Overlay.qml`) rather than a Rust async task so that the Qt event loop
/// remains the single source of concurrency in this backend.  Each timer tick
/// calls `demo_tick()` which advances the demo sequence by one step.

// ---------------------------------------------------------------------------
// cxx-qt bridge
// ---------------------------------------------------------------------------
#[cxx_qt::bridge(cxx_file_stem = "overlay_bridge")]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    /// The QML-visible overlay object.  Properties are backed by the inner
    /// `OverlayBridgeRust` struct; any write triggers an automatic
    /// `<prop>Changed` notify signal via the cxx-qt property system.
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        #[qproperty(QString, status_detail)]
        #[qproperty(QString, partial_transcript)]
        #[qproperty(QString, final_transcript)]
        #[qproperty(bool, expanded)]
        #[qproperty(bool, paused)]
        #[qproperty(QString, error_message)]
        type OverlayBridge = super::OverlayBridgeRust;
    }

    // Signals — emitted on meaningful state transitions.
    unsafe extern "RustQt" {
        /// Fired whenever the overlay reaches the `ready` state so QML
        /// can play a visual confirmation animation.
        #[qsignal]
        fn transcript_ready(self: Pin<&mut OverlayBridge>);

        /// Fired when an error is surfaced so QML can draw attention to it.
        #[qsignal]
        fn error_raised(self: Pin<&mut OverlayBridge>);
    }

    // Invokables — callable from QML controls.
    unsafe extern "RustQt" {
        /// Start or restart the overlay demo sequence.
        #[qinvokable]
        fn start_pipeline(self: Pin<&mut OverlayBridge>);

        /// Stop the current demo/capture session and return to idle.
        #[qinvokable]
        fn stop_pipeline(self: Pin<&mut OverlayBridge>);

        /// Toggle pause state.  Only valid while listening/demo is active.
        #[qinvokable]
        fn toggle_pause(self: Pin<&mut OverlayBridge>);

        /// Clear all transcript state and return to idle.
        #[qinvokable]
        fn clear_transcript(self: Pin<&mut OverlayBridge>);

        /// Collapse or expand the overlay window.
        #[qinvokable]
        fn set_expanded(self: Pin<&mut OverlayBridge>, expanded: bool);

        /// Open the settings panel (placeholder — not implemented yet).
        #[qinvokable]
        fn open_settings(self: Pin<&mut OverlayBridge>);

        /// Advance the demo sequence by one step.  Called by the QML Timer
        /// on each tick while the demo is active.
        #[qinvokable]
        fn demo_tick(self: Pin<&mut OverlayBridge>);

        // -----------------------------------------------------------------
        // STT pipeline wiring — called by the real audio/STT subsystem once
        // integrated.  The same contract as the Tauri `update_*` commands.
        // -----------------------------------------------------------------

        /// Feed a live partial transcript from the STT pipeline.
        #[qinvokable]
        fn apply_partial_transcript(self: Pin<&mut OverlayBridge>, text: &QString);

        /// Commit a final transcript and transition to `ready`.
        #[qinvokable]
        fn apply_final_transcript(self: Pin<&mut OverlayBridge>, text: &QString);

        /// Signal that the STT engine is processing the utterance.
        #[qinvokable]
        fn set_processing(self: Pin<&mut OverlayBridge>);

        /// Signal that a new utterance has started.
        #[qinvokable]
        fn set_listening(self: Pin<&mut OverlayBridge>);

        /// End the capture session and return to idle.
        #[qinvokable]
        fn stop_capture(self: Pin<&mut OverlayBridge>);
    }
}

// ---------------------------------------------------------------------------
// Inner Rust struct — owns all state
// ---------------------------------------------------------------------------

use cxx_qt_lib::QString;

/// Status tokens — mirrors the Tauri `OverlayStatus` enum but as a `QString`
/// so QML can bind directly without a converter.
mod status {
    pub const IDLE: &str = "idle";
    pub const LISTENING: &str = "listening";
    pub const PROCESSING: &str = "processing";
    pub const READY: &str = "ready";
    pub const ERROR: &str = "error";
}

#[derive(Default)]
pub struct OverlayBridgeRust {
    // ---------- QML-visible properties ----------
    status: QString,
    status_detail: QString,
    partial_transcript: QString,
    final_transcript: QString,
    expanded: bool,
    paused: bool,
    error_message: QString,

    // ---------- Internal demo driver state ----------
    /// `true` while the QML Timer is driving demo ticks.
    demo_active: bool,
    /// Index of the next demo step to apply.
    demo_step: usize,
    /// Incremented on stop/clear; QML Timer checks this to cancel stale runs.
    demo_generation: u64,
}

// Note: `cxx-qt-lib`'s `QString` implements `Default` as an empty string;
// we rely on that derived default for `OverlayBridgeRust`.

// ---------------------------------------------------------------------------
// Invokable implementations
// ---------------------------------------------------------------------------

impl qobject::OverlayBridge {
    fn start_pipeline(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().set_status(QString::from(status::LISTENING));
        self.as_mut()
            .set_status_detail(QString::from("Demo starting — watch the partial stream build up."));
        self.as_mut().set_partial_transcript(QString::from(""));
        self.as_mut().set_final_transcript(QString::from(""));
        self.as_mut().set_expanded(true);
        self.as_mut().set_paused(false);
        self.as_mut().set_error_message(QString::from(""));

        let gen = self.demo_generation() + 1;
        self.as_mut().set_demo_generation(gen);
        self.as_mut().set_demo_active(true);
        self.as_mut().set_demo_step(0);
    }

    fn stop_pipeline(mut self: core::pin::Pin<&mut Self>) {
        if *self.status() == QString::from(status::IDLE) {
            self.as_mut()
                .set_error_message(QString::from("Nothing is active to stop."));
            self.as_mut().set_status(QString::from(status::ERROR));
            self.as_mut()
                .set_status_detail(QString::from("Stop only applies while a session is active."));
            self.as_mut().set_expanded(true);
            let _ = self.as_mut().error_raised();
            return;
        }

        let gen = self.demo_generation() + 1;
        self.as_mut().set_demo_generation(gen);
        self.as_mut().set_demo_active(false);
        self.as_mut().set_status(QString::from(status::IDLE));
        self.as_mut().set_paused(false);
        self.as_mut().set_partial_transcript(QString::from(""));
        self.as_mut()
            .set_status_detail(QString::from("Capture stopped. Ready for the next session."));
        self.as_mut().set_error_message(QString::from(""));
    }

    fn toggle_pause(mut self: core::pin::Pin<&mut Self>) {
        if *self.status() != QString::from(status::LISTENING) {
            self.as_mut()
                .set_error_message(QString::from("Pause is only available while listening."));
            self.as_mut().set_status(QString::from(status::ERROR));
            self.as_mut()
                .set_status_detail(QString::from("Pause/resume applies during the demo only."));
            self.as_mut().set_expanded(true);
            let _ = self.as_mut().error_raised();
            return;
        }

        let now_paused = !self.paused();
        self.as_mut().set_paused(now_paused);
        let detail = if now_paused {
            "Demo paused. Resume when ready."
        } else {
            "Listening for provisional words."
        };
        self.as_mut().set_status_detail(QString::from(detail));
        self.as_mut().set_error_message(QString::from(""));
    }

    fn clear_transcript(mut self: core::pin::Pin<&mut Self>) {
        let gen = self.demo_generation() + 1;
        self.as_mut().set_demo_generation(gen);
        self.as_mut().set_demo_active(false);
        self.as_mut().set_demo_step(0);
        self.as_mut().set_status(QString::from(status::IDLE));
        self.as_mut().set_paused(false);
        self.as_mut().set_partial_transcript(QString::from(""));
        self.as_mut().set_final_transcript(QString::from(""));
        self.as_mut()
            .set_status_detail(QString::from("Transcript cleared. Ready for a new session."));
        self.as_mut().set_error_message(QString::from(""));
    }

    fn set_expanded(mut self: core::pin::Pin<&mut Self>, expanded: bool) {
        self.as_mut().set_expanded(expanded);
        if !expanded && *self.status() == QString::from(status::IDLE) {
            self.as_mut()
                .set_status_detail(QString::from("Overlay shell ready. Expand to inspect."));
        }
    }

    fn open_settings(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().set_expanded(true);
        self.as_mut().set_status_detail(QString::from(
            "Settings panel — not yet implemented in this tranche.",
        ));
    }

    /// Advance the demo by one step.  Called by the QML `Timer` on each tick.
    fn demo_tick(mut self: core::pin::Pin<&mut Self>) {
        if !self.demo_active() || *self.paused() {
            return;
        }

        use crate::demo::{demo_script, DemoStep};
        let script = demo_script();
        let idx = *self.demo_step();

        if idx >= script.len() {
            // Demo finished.
            self.as_mut().set_demo_active(false);
            return;
        }

        match &script[idx] {
            DemoStep::Partial(text) => {
                self.as_mut()
                    .set_partial_transcript(QString::from(*text));
                self.as_mut()
                    .set_status_detail(QString::from("Live transcription…"));
            }
            DemoStep::Final(text) => {
                self.as_mut().set_partial_transcript(QString::from(""));
                self.as_mut()
                    .set_final_transcript(QString::from(*text));
                self.as_mut().set_status(QString::from(status::READY));
                self.as_mut()
                    .set_status_detail(QString::from("Transcription complete."));
                self.as_mut().set_demo_active(false);
                let _ = self.as_mut().transcript_ready();
            }
        }

        let next = idx + 1;
        self.as_mut().set_demo_step(next);
    }

    // -----------------------------------------------------------------
    // STT pipeline wiring
    // -----------------------------------------------------------------

    fn apply_partial_transcript(mut self: core::pin::Pin<&mut Self>, text: &QString) {
        self.as_mut().set_partial_transcript(text.clone());
        self.as_mut().set_status(QString::from(status::LISTENING));
        self.as_mut().set_paused(false);
        self.as_mut().set_expanded(true);
        self.as_mut()
            .set_status_detail(QString::from("Streaming partial words from the STT pipeline."));
        self.as_mut().set_error_message(QString::from(""));
    }

    fn apply_final_transcript(mut self: core::pin::Pin<&mut Self>, text: &QString) {
        self.as_mut().set_partial_transcript(QString::from(""));
        self.as_mut().set_final_transcript(text.clone());
        self.as_mut().set_status(QString::from(status::READY));
        self.as_mut().set_paused(false);
        self.as_mut().set_expanded(true);
        self.as_mut()
            .set_status_detail(QString::from("Final transcript ready."));
        self.as_mut().set_error_message(QString::from(""));
        let _ = self.as_mut().transcript_ready();
    }

    fn set_processing(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().set_status(QString::from(status::PROCESSING));
        self.as_mut().set_paused(false);
        self.as_mut().set_expanded(true);
        self.as_mut()
            .set_status_detail(QString::from("Processing the utterance."));
    }

    fn set_listening(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().set_status(QString::from(status::LISTENING));
        self.as_mut().set_partial_transcript(QString::from(""));
        self.as_mut().set_final_transcript(QString::from(""));
        self.as_mut().set_paused(false);
        self.as_mut().set_expanded(true);
        self.as_mut()
            .set_status_detail(QString::from("Listening for speech."));
    }

    fn stop_capture(mut self: core::pin::Pin<&mut Self>) {
        let gen = self.demo_generation() + 1;
        self.as_mut().set_demo_generation(gen);
        self.as_mut().set_demo_active(false);
        self.as_mut().set_status(QString::from(status::IDLE));
        self.as_mut().set_paused(false);
        self.as_mut().set_partial_transcript(QString::from(""));
        self.as_mut().set_final_transcript(QString::from(""));
        self.as_mut()
            .set_status_detail(QString::from("Capture stopped."));
        self.as_mut().set_error_message(QString::from(""));
    }
}

// ---------------------------------------------------------------------------
// Private helpers — not exposed to QML
// ---------------------------------------------------------------------------

impl OverlayBridgeRust {
    fn demo_active(&self) -> bool {
        self.demo_active
    }

    fn set_demo_active(&mut self, v: bool) {
        self.demo_active = v;
    }

    fn demo_step(&self) -> &usize {
        &self.demo_step
    }

    fn set_demo_step(&mut self, v: usize) {
        self.demo_step = v;
    }

    fn demo_generation(&self) -> u64 {
        self.demo_generation
    }

    fn set_demo_generation(&mut self, v: u64) {
        self.demo_generation = v;
    }
}
