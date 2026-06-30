use crate::contract::{OverlaySnapshot, OverlayStatus};
use crate::demo::DemoStep;

#[derive(Debug, Default)]
pub struct OverlayModel {
    snapshot: OverlaySnapshot,
    /// Monotonically increasing token that is bumped on every stop/clear.
    /// The demo async task captures the token at start and exits as soon as
    /// the live value no longer matches, preventing stale demo tasks from
    /// interfering with a freshly started session.
    demo_token: u64,
}

impl OverlayModel {
    pub fn snapshot(&self) -> OverlaySnapshot {
        self.snapshot.clone()
    }

    pub fn set_status(&mut self, status: OverlayStatus, detail: String) -> OverlaySnapshot {
        self.snapshot.status = status;
        self.snapshot.status_detail = detail;
        self.snapshot()
    }

    pub fn is_paused(&self) -> bool {
        self.snapshot.paused
    }

    pub fn set_expanded(&mut self, expanded: bool) -> OverlaySnapshot {
        self.snapshot.expanded = expanded;

        if !expanded && self.snapshot.status == OverlayStatus::Idle {
            self.snapshot.status_detail =
                "Overlay shell ready. Expand to inspect the seam.".to_string();
        }

        self.snapshot()
    }

    pub fn update_partial(&mut self, text: String) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Listening;
        self.snapshot.partial_transcript = text;
        self.snapshot.status_detail = "Live transcription...".to_string();
        self.snapshot()
    }

    pub fn update_final(&mut self, text: String) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Ready;
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript = text;
        self.snapshot.status_detail = "Transcription complete.".to_string();
        self.snapshot()
    }

    pub fn reset_to_idle(&mut self, detail: String) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Idle;
        self.snapshot.status_detail = detail;
        self.snapshot.partial_transcript.clear();
        self.snapshot.error_message = None;
        self.snapshot()
    }

    pub fn toggle_pause(&mut self) -> OverlaySnapshot {
        if self.snapshot.status != OverlayStatus::Listening {
            return self.reject_command(
                "Pause/resume is only available during the listening demo phase.",
                "Pause is a placeholder seam until real capture wiring lands.",
            );
        }

        self.snapshot.paused = !self.snapshot.paused;
        self.snapshot.status_detail = if self.snapshot.paused {
            "Demo paused. Resume when you want the partial stream to continue.".to_string()
        } else {
            "Listening for provisional words from the demo driver.".to_string()
        };
        self.snapshot.error_message = None;
        self.snapshot()
    }

    pub fn stop(&mut self) -> OverlaySnapshot {
        if self.snapshot.status != OverlayStatus::Listening
            && self.snapshot.status != OverlayStatus::Processing
        {
            return self.reject_command(
                "Nothing is active to stop.",
                "Stop only applies while the demo is simulating live capture.",
            );
        }

        self.demo_token += 1;
        self.snapshot.status = OverlayStatus::Idle;
        self.snapshot.paused = false;
        self.snapshot.partial_transcript.clear();
        self.snapshot.status_detail =
            "Capture stopped. Expand again or rerun the demo when ready.".to_string();
        self.snapshot.error_message = None;
        self.snapshot()
    }

    pub fn clear(&mut self) -> OverlaySnapshot {
        self.demo_token += 1;
        let expanded = self.snapshot.expanded;
        self.snapshot = OverlaySnapshot {
            expanded,
            status_detail: "Transcript cleared. Demo seam remains ready.".to_string(),
            ..OverlaySnapshot::default()
        };
        self.snapshot()
    }

    pub fn open_settings_placeholder(&mut self) -> OverlaySnapshot {
        self.snapshot.expanded = true;
        self.snapshot.status_detail =
            "Settings stay out of scope in this tranche, but the command seam is in place."
                .to_string();
        if self.snapshot.status != OverlayStatus::Error {
            self.snapshot.error_message = None;
        }
        self.snapshot()
    }

    /// Apply a live partial transcript update from the STT pipeline.
    /// Keeps the shell in Listening state and updates the provisional text.
    pub fn apply_partial_transcript(
        &mut self,
        text: &str,
        status_detail: Option<&str>,
    ) -> OverlaySnapshot {
        self.snapshot.partial_transcript = text.to_string();
        self.snapshot.status = OverlayStatus::Listening;
        self.snapshot.paused = false;
        if let Some(detail) = status_detail {
            self.snapshot.status_detail = detail.to_string();
        } else {
            self.snapshot.status_detail =
                "Streaming partial words from the STT pipeline.".to_string();
        }
        self.snapshot.error_message = None;
        self.snapshot.expanded = true;
        self.snapshot()
    }

    /// Promote the current partial transcript to final and transition to Ready.
    /// Called when the STT pipeline commits an utterance.
    pub fn apply_final_transcript(
        &mut self,
        text: &str,
        status_detail: Option<&str>,
    ) -> OverlaySnapshot {
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript = text.to_string();
        self.snapshot.status = OverlayStatus::Ready;
        self.snapshot.paused = false;
        if let Some(detail) = status_detail {
            self.snapshot.status_detail = detail.to_string();
        } else {
            self.snapshot.status_detail =
                "Final transcript staged. Real injection wiring lands in a later tranche."
                    .to_string();
        }
        self.snapshot.error_message = None;
        self.snapshot.expanded = true;
        self.snapshot()
    }

    /// Transition to Processing state (STT pipeline is finalizing the utterance).
    pub fn apply_processing_state(&mut self, status_detail: Option<&str>) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Processing;
        self.snapshot.paused = false;
        if let Some(detail) = status_detail {
            self.snapshot.status_detail = detail.to_string();
        } else {
            self.snapshot.status_detail =
                "Processing the utterance into a committed transcript.".to_string();
        }
        self.snapshot.expanded = true;
        self.snapshot()
    }

    /// Transition to Listening state (new utterance started).
    pub fn apply_listening_state(&mut self, status_detail: Option<&str>) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Listening;
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript.clear();
        self.snapshot.paused = false;
        if let Some(detail) = status_detail {
            self.snapshot.status_detail = detail.to_string();
        } else {
            self.snapshot.status_detail = "Listening for speech.".to_string();
        }
        self.snapshot.expanded = true;
        self.snapshot()
    }

    /// Stop capture and return to Idle, clearing all transcript state.
    /// Unlike `stop()` which increments the demo token, this is used by the real pipeline.
    pub fn stop_capture(&mut self) -> OverlaySnapshot {
        self.demo_token += 1;
        self.snapshot.status = OverlayStatus::Idle;
        self.snapshot.paused = false;
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript.clear();
        self.snapshot.status_detail =
            "Capture stopped. The seam is ready for the next session.".to_string();
        self.snapshot.error_message = None;
        self.snapshot()
    }

    /// Start the overlay demo and return the captured demo token together with
    /// the initial snapshot. The caller spawns an async task that drives the
    /// demo steps; it must exit when `current_demo_token()` no longer equals
    /// the returned token.
    pub fn start_demo(&mut self) -> (u64, OverlaySnapshot) {
        self.demo_token += 1;
        let token = self.demo_token;
        self.snapshot.status = OverlayStatus::Listening;
        self.snapshot.paused = false;
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript.clear();
        self.snapshot.expanded = true;
        self.snapshot.status_detail =
            "Demo starting — watch the partial stream build up.".to_string();
        self.snapshot.error_message = None;
        (token, self.snapshot())
    }

    /// Apply a single demo step and return the resulting snapshot.
    pub fn apply_demo_step(&mut self, step: &DemoStep) -> OverlaySnapshot {
        match step {
            DemoStep::Partial(text) => self.update_partial(text.clone()),
            DemoStep::Final(text) => self.update_final(text.clone()),
            DemoStep::Wait(_) => self.snapshot(),
        }
    }

    /// Return the current demo token. The demo driver uses this to detect
    /// whether a stop/clear has made the running demo stale.
    pub fn current_demo_token(&self) -> u64 {
        self.demo_token
    }

    fn reject_command(&mut self, message: &str, detail: &str) -> OverlaySnapshot {
        self.demo_token += 1;
        self.snapshot.expanded = true;
        self.snapshot.status = OverlayStatus::Error;
        self.snapshot.paused = false;
        self.snapshot.status_detail = detail.to_string();
        self.snapshot.error_message = Some(message.to_string());
        self.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::demo_script;

    #[test]
    fn starts_idle_and_collapsed() {
        let model = OverlayModel::default();

        assert_eq!(model.snapshot.status, OverlayStatus::Idle);
        assert!(!model.snapshot.expanded);
        assert!(model.snapshot.partial_transcript.is_empty());
        assert!(model.snapshot.final_transcript.is_empty());
    }

    #[test]
    fn stop_from_idle_surfaces_error_state() {
        let mut model = OverlayModel::default();
        let snapshot = model.stop();

        assert_eq!(snapshot.status, OverlayStatus::Error);
        assert_eq!(
            snapshot.error_message.as_deref(),
            Some("Nothing is active to stop."),
        );
    }

    #[test]
    fn demo_script_reaches_ready_state_with_final_text() {
        let mut model = OverlayModel::default();
        let (_token, start_snapshot) = model.start_demo();

        assert_eq!(start_snapshot.status, OverlayStatus::Listening);

        let mut last_snapshot = start_snapshot;
        for step in demo_script() {
            last_snapshot = model.apply_demo_step(&step);
        }

        assert_eq!(last_snapshot.status, OverlayStatus::Ready);
        assert!(last_snapshot.partial_transcript.is_empty());
        assert!(last_snapshot
            .final_transcript
            .contains("Streaming partials"));
    }

    #[test]
    fn pause_round_trip_keeps_demo_in_listening_state() {
        let mut model = OverlayModel::default();
        model.start_demo();

        let paused = model.toggle_pause();
        assert_eq!(paused.status, OverlayStatus::Listening);
        assert!(paused.paused);

        let resumed = model.toggle_pause();
        assert_eq!(resumed.status, OverlayStatus::Listening);
        assert!(!resumed.paused);
    }

    #[test]
    fn apply_partial_transcript_updates_text_and_keeps_listening() {
        let mut model = OverlayModel::default();
        model.apply_listening_state(None);

        let snap1 = model.apply_partial_transcript("hello", None);
        assert_eq!(snap1.partial_transcript, "hello");
        assert_eq!(snap1.status, OverlayStatus::Listening);
        assert!(snap1.final_transcript.is_empty());

        let snap2 = model.apply_partial_transcript("hello world", None);
        assert_eq!(snap2.partial_transcript, "hello world");
        assert_eq!(snap2.status, OverlayStatus::Listening);
    }

    #[test]
    fn apply_final_transcript_moves_partial_to_final_and_transitions_to_ready() {
        let mut model = OverlayModel::default();
        model.apply_listening_state(None);
        model.apply_partial_transcript("hello world", None);

        let snap = model.apply_final_transcript("hello world", None);
        assert!(snap.partial_transcript.is_empty());
        assert_eq!(snap.final_transcript, "hello world");
        assert_eq!(snap.status, OverlayStatus::Ready);
    }

    #[test]
    fn stop_capture_clears_all_transcript_state() {
        let mut model = OverlayModel::default();
        model.apply_listening_state(None);
        model.apply_partial_transcript("partial text", None);

        let snap = model.stop_capture();
        assert_eq!(snap.status, OverlayStatus::Idle);
        assert!(snap.partial_transcript.is_empty());
        assert!(snap.final_transcript.is_empty());
        assert!(snap.error_message.is_none());
    }

    #[test]
    fn stop_capture_bumps_demo_token_and_stops_demo_driver() {
        let mut model = OverlayModel::default();
        let (token, _snap) = model.start_demo();
        assert_eq!(model.current_demo_token(), token);

        // stop_capture must bump the token so the demo driver exits its loop
        model.stop_capture();
        assert_eq!(model.current_demo_token(), token + 1);
    }

    #[test]
    fn pipeline_transitions_reset_paused_flag() {
        let mut model = OverlayModel::default();

        // Simulate paused state from demo
        model.start_demo();
        model.toggle_pause(); // now paused
        assert!(model.snapshot.paused);

        // Any pipeline transition must clear paused so real capture is not stuck
        let snap1 = model.apply_partial_transcript("hello", None);
        assert!(!snap1.paused);

        model.toggle_pause(); // paused again
        let snap2 = model.apply_final_transcript("hello world", None);
        assert!(!snap2.paused);

        model.toggle_pause(); // paused again
        let snap3 = model.apply_processing_state(None);
        assert!(!snap3.paused);

        model.toggle_pause(); // paused again
        let snap4 = model.apply_listening_state(None);
        assert!(!snap4.paused);
    }
}
