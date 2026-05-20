use crate::contract::{OverlaySnapshot, OverlayStatus};

#[derive(Debug, Default)]
pub struct OverlayModel {
    snapshot: OverlaySnapshot,
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
                "Pause/resume is only available during the listening phase.",
                "Pause is a placeholder seam until real capture wiring lands.",
            );
        }

        self.snapshot.paused = !self.snapshot.paused;
        self.snapshot.status_detail = if self.snapshot.paused {
            "Paused. Resume when ready to continue.".to_string()
        } else {
            "Listening for speech.".to_string()
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
                "Stop only applies while the pipeline is active.",
            );
        }

        self.snapshot.status = OverlayStatus::Idle;
        self.snapshot.paused = false;
        self.snapshot.partial_transcript.clear();
        self.snapshot.status_detail =
            "Capture stopped. Expand again or restart the pipeline when ready.".to_string();
        self.snapshot.error_message = None;
        self.snapshot()
    }

    pub fn clear(&mut self) -> OverlaySnapshot {
        let expanded = self.snapshot.expanded;
        self.snapshot = OverlaySnapshot {
            expanded,
            status_detail: "Transcript cleared.".to_string(),
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

    fn reject_command(&mut self, message: &str, detail: &str) -> OverlaySnapshot {
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
    fn update_partial_sets_listening_state() {
        let mut model = OverlayModel::default();
        let snap = model.update_partial("hello world".to_string());

        assert_eq!(snap.status, OverlayStatus::Listening);
        assert_eq!(snap.partial_transcript, "hello world");
        assert!(snap.final_transcript.is_empty());
    }

    #[test]
    fn update_final_moves_to_ready_and_clears_partial() {
        let mut model = OverlayModel::default();
        model.update_partial("hello".to_string());
        let snap = model.update_final("hello world".to_string());

        assert_eq!(snap.status, OverlayStatus::Ready);
        assert!(snap.partial_transcript.is_empty());
        assert_eq!(snap.final_transcript, "hello world");
    }

    #[test]
    fn reset_to_idle_clears_transcript_state() {
        let mut model = OverlayModel::default();
        model.update_partial("partial text".to_string());

        let snap = model.reset_to_idle("Pipeline stopped.".to_string());
        assert_eq!(snap.status, OverlayStatus::Idle);
        assert!(snap.partial_transcript.is_empty());
        assert!(snap.error_message.is_none());
    }

    #[test]
    fn pause_round_trip_keeps_listening_state() {
        let mut model = OverlayModel::default();
        model.update_partial("hello".to_string()); // put into Listening

        let paused = model.toggle_pause();
        assert_eq!(paused.status, OverlayStatus::Listening);
        assert!(paused.paused);

        let resumed = model.toggle_pause();
        assert_eq!(resumed.status, OverlayStatus::Listening);
        assert!(!resumed.paused);
    }
}
