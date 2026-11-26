//! Conversion from neutral player snapshots to the core's persisted progress contract.
use ntsmb_core::{PlaybackState, PlaybackStateRepository};
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

/// Persist a snapshot using only the core repository contract.
/// Returns false when the snapshot cannot represent a valid resume point.
pub fn persist_snapshot<R: PlaybackStateRepository>(
    repository: &R,
    snapshot: &PlayerSnapshot,
    media_id: impl Into<String>,
) -> Result<bool, String> {
    let Some(state) = snapshot.to_playback_state(media_id) else { return Ok(false); };
    repository.save_state(&state)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    #[derive(Default)]
    struct MemoryRepository(Mutex<HashMap<String, PlaybackState>>);

    impl PlaybackStateRepository for MemoryRepository {
        fn load_state(&self, id: &str) -> Result<Option<PlaybackState>, String> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }
        fn save_state(&self, state: &PlaybackState) -> Result<(), String> {
            self.0.lock().unwrap().insert(state.media_id.clone(), state.clone());
            Ok(())
        }
        fn delete_state(&self, id: &str) -> Result<(), String> {
            self.0.lock().unwrap().remove(id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailingRepository;
    impl PlaybackStateRepository for FailingRepository {
        fn load_state(&self, _: &str) -> Result<Option<PlaybackState>, String> { Ok(None) }
        fn save_state(&self, _: &PlaybackState) -> Result<(), String> { Err("disk unavailable".into()) }
        fn delete_state(&self, _: &str) -> Result<(), String> { Ok(()) }
    }

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
        assert!(zero_duration.to_playback_state("movie").is_none());
    }

    #[test]
    fn persist_snapshot_saves_valid_record_and_ignores_invalid_snapshots() {
        let repository = MemoryRepository::default();
        let snapshot = PlayerSnapshot { status: PlayerStatus::Playing, position_ms: 12_345,
            duration_ms: Some(90_000), error: None };
        assert!(persist_snapshot(&repository, &snapshot, "movie-id").unwrap());
        assert_eq!(repository.load_state("movie-id").unwrap(), Some(PlaybackState {
            media_id: "movie-id".into(), position_ms: 12_345, duration_ms: Some(90_000),
            completed: false,
        }));
        assert!(!persist_snapshot(&repository, &PlayerSnapshot::default(), "invalid").unwrap());
        assert_eq!(repository.load_state("invalid").unwrap(), None);
    }

    #[test]
    fn persist_snapshot_propagates_repository_errors() {
        let snapshot = PlayerSnapshot { status: PlayerStatus::Playing, position_ms: 10,
            duration_ms: Some(100), error: None };
        assert_eq!(persist_snapshot(&FailingRepository, &snapshot, "film"),
            Err("disk unavailable".into()));
    }
}
