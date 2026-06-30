//! Overlay state machine for the ColdVox Tauri shell.
//!
//! Framework-agnostic model backing every `#[tauri::command]` in `lib.rs`.
//! Owns a single [`OverlaySnapshot`] and exposes the state transitions the
//! React overlay (and the real STT pipeline) drive:
//!
//! - UI commands: `set_expanded`, `toggle_pause`, `clear`, `open_settings_placeholder`
//! - Pipeline-driven transitions: `update_partial`, `update_final`,
//!   `apply_error`, `reset_to_idle`
//! - External STT-driver seam: `apply_partial_transcript`,
//!   `apply_final_transcript`, `apply_processing_state`,
//!   `apply_listening_state`, `stop_capture`

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
        self.snapshot.status_detail = "Streaming partial words from the STT pipeline.".to_string();
        // Clear any stale error badge once the pipeline is producing partials
        // again — otherwise a prior reject_command error lingers on a healthy
        // Listening snapshot.
        self.snapshot.error_message = None;
        self.snapshot()
    }

    pub fn update_final(&mut self, text: String) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Ready;
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript = text;
        self.snapshot.status_detail = "Transcription complete.".to_string();
        self.snapshot.error_message = None;
        self.snapshot()
    }

    pub fn reset_to_idle(&mut self, detail: String) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Idle;
        self.snapshot.status_detail = detail;
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript.clear();
        self.snapshot.error_message = None;
        self.snapshot()
    }

    pub fn toggle_pause(&mut self) -> OverlaySnapshot {
        if self.snapshot.status != OverlayStatus::Listening {
            return self.reject_command(
                "Pause/resume is only available while listening.",
                "Pause is a placeholder seam until capture wiring exposes a real pause knob.",
            );
        }

        self.snapshot.paused = !self.snapshot.paused;
        self.snapshot.status_detail = if self.snapshot.paused {
            "Paused. Resume to continue the partial stream.".to_string()
        } else {
            "Listening for provisional words from the pipeline.".to_string()
        };
        self.snapshot.error_message = None;
        self.snapshot()
    }

    pub fn clear(&mut self) -> OverlaySnapshot {
        // Only clear transcript + error fields. Forcing `status = Idle` here
        // (as the prior `..OverlaySnapshot::default()` did) desyncs the model
        // from a still-running runtime: the tray would disable Stop and the
        // shortcut would route a toggle to Start while capture is live.
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript.clear();
        self.snapshot.error_message = None;
        self.snapshot.status_detail = "Transcript cleared. Capture state is unchanged.".to_string();
        self.snapshot()
    }

    pub fn open_settings_placeholder(&mut self) -> OverlaySnapshot {
        self.snapshot.expanded = true;
        self.snapshot.status_detail =
            "Settings window is a placeholder in this tranche; the command seam is in place."
                .to_string();
        if self.snapshot.status != OverlayStatus::Error {
            self.snapshot.error_message = None;
        }
        self.snapshot()
    }

    /// Apply a pipeline-level error. Sets BOTH `status_detail` and
    /// `error_message` so the React overlay's error badge (which reads
    /// `errorMessage`) actually surfaces the STT failure text. `set_status`
    /// alone only updates `status_detail`, leaving the badge blank.
    pub fn apply_error(&mut self, message: String) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Error;
        self.snapshot.paused = false;
        self.snapshot.status_detail = message.clone();
        self.snapshot.error_message = Some(message);
        self.snapshot.expanded = true;
        self.snapshot()
    }

    // ── External STT-driver seam ────────────────────────────────────────────

    /// Apply a live partial transcript update from the STT pipeline.
    pub fn apply_partial_transcript(
        &mut self,
        text: &str,
        status_detail: Option<&str>,
    ) -> OverlaySnapshot {
        self.snapshot.partial_transcript = text.to_string();
        self.snapshot.status = OverlayStatus::Listening;
        self.snapshot.paused = false;
        self.snapshot.status_detail = status_detail
            .map(str::to_string)
            .unwrap_or_else(|| "Streaming partial words from the STT pipeline.".to_string());
        self.snapshot.error_message = None;
        self.snapshot.expanded = true;
        self.snapshot()
    }

    /// Promote the current partial transcript to final and transition to Ready.
    pub fn apply_final_transcript(
        &mut self,
        text: &str,
        status_detail: Option<&str>,
    ) -> OverlaySnapshot {
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript = text.to_string();
        self.snapshot.status = OverlayStatus::Ready;
        self.snapshot.paused = false;
        self.snapshot.status_detail = status_detail
            .map(str::to_string)
            .unwrap_or_else(|| "Final transcript staged.".to_string());
        self.snapshot.error_message = None;
        self.snapshot.expanded = true;
        self.snapshot()
    }

    /// Transition to Processing state (STT pipeline is finalizing the utterance).
    pub fn apply_processing_state(&mut self, status_detail: Option<&str>) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Processing;
        self.snapshot.paused = false;
        self.snapshot.status_detail = status_detail
            .map(str::to_string)
            .unwrap_or_else(|| "Processing the utterance into a committed transcript.".to_string());
        self.snapshot.error_message = None;
        self.snapshot.expanded = true;
        self.snapshot()
    }

    /// Transition to Listening state (new utterance started).
    pub fn apply_listening_state(&mut self, status_detail: Option<&str>) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Listening;
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript.clear();
        self.snapshot.paused = false;
        self.snapshot.status_detail = status_detail
            .map(str::to_string)
            .unwrap_or_else(|| "Listening for speech.".to_string());
        self.snapshot.error_message = None;
        self.snapshot.expanded = true;
        self.snapshot()
    }

    /// Stop real capture and return to Idle, clearing all transcript state.
    pub fn stop_capture(&mut self) -> OverlaySnapshot {
        self.snapshot.status = OverlayStatus::Idle;
        self.snapshot.paused = false;
        self.snapshot.partial_transcript.clear();
        self.snapshot.final_transcript.clear();
        self.snapshot.status_detail =
            "Capture stopped. The seam is ready for the next session.".to_string();
        self.snapshot.error_message = None;
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
    fn toggle_pause_only_works_while_listening() {
        let mut model = OverlayModel::default();
        // From Idle -> reject
        let snap = model.toggle_pause();
        assert_eq!(snap.status, OverlayStatus::Error);

        // From Listening -> paused
        model.apply_listening_state(None);
        let paused = model.toggle_pause();
        assert_eq!(paused.status, OverlayStatus::Listening);
        assert!(paused.paused);

        // Resume
        let resumed = model.toggle_pause();
        assert_eq!(resumed.status, OverlayStatus::Listening);
        assert!(!resumed.paused);
    }

    #[test]
    fn apply_partial_then_final_transitions_correctly() {
        let mut model = OverlayModel::default();
        model.apply_listening_state(None);

        let snap = model.apply_partial_transcript("hello world", None);
        assert_eq!(snap.partial_transcript, "hello world");
        assert_eq!(snap.status, OverlayStatus::Listening);

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
    fn pipeline_transitions_reset_paused_flag() {
        let mut model = OverlayModel::default();
        model.apply_listening_state(None);
        model.toggle_pause();
        assert!(model.snapshot.paused);

        assert!(!model.apply_partial_transcript("hello", None).paused);
        model.toggle_pause();
        assert!(!model.apply_final_transcript("hello world", None).paused);
        model.toggle_pause();
        assert!(!model.apply_processing_state(None).paused);
        model.toggle_pause();
        assert!(!model.apply_listening_state(None).paused);
    }

    #[test]
    fn clear_keeps_pipeline_status() {
        // `clear()` must not force status back to Idle while the pipeline is
        // running — that would desync the tray/shortcut from the live runtime.
        let mut model = OverlayModel::default();
        model.apply_listening_state(None);
        model.set_expanded(true);
        let snap = model.clear();
        assert!(snap.expanded);
        assert_eq!(snap.status, OverlayStatus::Listening);
        assert!(snap.partial_transcript.is_empty());
        assert!(snap.final_transcript.is_empty());
        assert!(snap.error_message.is_none());
    }

    #[test]
    fn apply_error_sets_both_status_detail_and_error_message() {
        let mut model = OverlayModel::default();
        let snap = model.apply_error("STT plugin crashed".to_string());
        assert_eq!(snap.status, OverlayStatus::Error);
        assert_eq!(snap.status_detail, "STT plugin crashed");
        assert_eq!(snap.error_message.as_deref(), Some("STT plugin crashed"));
    }

    #[test]
    fn success_transitions_clear_stale_error_message() {
        let mut model = OverlayModel::default();
        model.apply_error("boom".to_string());
        assert!(model.snapshot().error_message.is_some());

        assert!(model.apply_listening_state(None).error_message.is_none());
        model.apply_error("boom2".to_string());
        assert!(model.apply_processing_state(None).error_message.is_none());
        model.apply_error("boom3".to_string());
        assert!(model
            .update_partial("hi".to_string())
            .error_message
            .is_none());
        model.apply_error("boom4".to_string());
        assert!(model.update_final("hi".to_string()).error_message.is_none());
    }
}
