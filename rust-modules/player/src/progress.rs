//! Conversion from neutral player snapshots to the core's persisted progress contract.
use ntsmb_core::PlaybackState;
use crate::{PlayerSnapshot, PlayerStatus};

impl PlayerSnapshot {
    /// Produce a persistable progress record when a real playback position is available.
    /// A stopped session is marked complete only when its final position reached its duration.
    pub fn to_playback_state(&self, media_id: impl Into<String>) -> Option<PlaybackState> {
        let duration_ms = self.duration_ms?;
        // A zero-length duration is a probe/readout placeholder, not a meaningful resume record.
        if duration_ms == 0 || self.status == PlayerStatus::Empty || self.status == PlayerStatus::Ready {
            return None;
        }
        let position_ms = self.position_ms.min(duration_ms);
        Some(PlaybackState {
            media_id: media_id.into(),
            position_ms,
            duration_ms: Some(duration_ms),
            completed: self.status == PlayerStatus::Stopped && duration_ms > 0
                && position_ms >= duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_session_exports_resume_position_without_marking_complete() {
        let state = PlayerSnapshot { status: PlayerStatus::Playing, position_ms: 42_000,
            duration_ms: Some(100_000), error: None }
            .to_playback_state("movie").unwrap();
        assert_eq!(state.media_id, "movie");
        assert_eq!(state.position_ms, 42_000);
        assert!(!state.completed);
    }

    #[test]
    fn only_a_finished_session_at_duration_is_complete() {
        let state = PlayerSnapshot { status: PlayerStatus::Stopped, position_ms: 100_000,
            duration_ms: Some(100_000), error: None }
            .to_playback_state("movie").unwrap();
        assert!(state.completed);
        let stopped_early = PlayerSnapshot { status: PlayerStatus::Stopped, position_ms: 20_000,
            duration_ms: Some(100_000), error: None }
            .to_playback_state("movie").unwrap();
        assert!(!stopped_early.completed);
    }

    #[test]
    fn unknown_duration_and_unstarted_sessions_are_not_persisted() {
        assert!(PlayerSnapshot::default().to_playback_state("movie").is_none());
        let ready = PlayerSnapshot { status: PlayerStatus::Ready, position_ms: 0,
            duration_ms: Some(100), error: None };
        assert!(ready.to_playback_state("movie").is_none());
        let zero_duration = PlayerSnapshot { status: PlayerStatus::Playing, position_ms: 5,
            duration_ms: Some(0), error: None };
        assert!(zero_duration.to_playback_state("movie").is_none(),
            "a zero duration is not a valid resume point");
    }
}
