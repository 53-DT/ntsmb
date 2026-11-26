//! Backend-neutral player session and control policy for NTSMB.
//!
//! This crate deliberately knows nothing about SMB, Jellyfin, FinNative, webOS, FFmpeg,
//! rendering, or application navigation. It consumes a neutral PlaybackSource and emits
//! commands for an engine adapter to execute. The existing native playback engine remains
//! behind that adapter while the migration proceeds.

use ntsmb_core::PlaybackSource;

/// Logical state of a playback session, independent of the decoder implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerStatus {
    #[default]
    Empty,
    Ready,
    Playing,
    Paused,
    Stopped,
    Failed,
}

/// Commands emitted by the neutral controller for the platform playback adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerCommand {
    Load(PlaybackSource),
    Play,
    Pause,
    SeekTo { position_ms: u64 },
    Stop,
}

/// A backend-neutral snapshot suitable for persistence or UI presentation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerSnapshot {
    pub status: PlayerStatus,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// Coordinates user intent and playback readouts without owning a decoder or backend resource.
#[derive(Debug, Clone, Default)]
pub struct PlayerController {
    source: Option<PlaybackSource>,
    snapshot: PlayerSnapshot,
}

impl PlayerController {
    pub fn new() -> Self { Self::default() }

    pub fn source(&self) -> Option<&PlaybackSource> { self.source.as_ref() }
    pub fn snapshot(&self) -> &PlayerSnapshot { &self.snapshot }

    /// Install a new neutral source and return the load command for the engine adapter.
    pub fn load(&mut self, source: PlaybackSource) -> PlayerCommand {
        self.source = Some(source.clone());
        self.snapshot = PlayerSnapshot {
            status: PlayerStatus::Ready,
            position_ms: 0,
            duration_ms: None,
            error: None,
        };
        PlayerCommand::Load(source)
    }

    pub fn play(&mut self) -> Option<PlayerCommand> {
        match self.snapshot.status {
            PlayerStatus::Ready | PlayerStatus::Paused | PlayerStatus::Stopped => {
                self.snapshot.status = PlayerStatus::Playing;
                Some(PlayerCommand::Play)
            }
            _ => None,
        }
    }

    pub fn pause(&mut self) -> Option<PlayerCommand> {
        if self.snapshot.status != PlayerStatus::Playing { return None; }
        self.snapshot.status = PlayerStatus::Paused;
        Some(PlayerCommand::Pause)
    }

    pub fn seek_to(&mut self, position_ms: u64) -> Option<PlayerCommand> {
        if self.source.is_none() || self.snapshot.status == PlayerStatus::Empty
            || self.snapshot.status == PlayerStatus::Failed {
            return None;
        }
        let target = self.snapshot.duration_ms.map_or(position_ms, |duration| position_ms.min(duration));
        self.snapshot.position_ms = target;
        Some(PlayerCommand::SeekTo { position_ms: target })
    }

    /// Accept a readout from the engine. Duration is optional until probing completes.
    pub fn report_position(&mut self, position_ms: u64, duration_ms: Option<u64>) {
        // Readouts are accepted only while this session can still make progress. A delayed
        // position callback must not overwrite the final position after finish/stop or mutate a
        // failed session while the native engine is tearing down.
        if self.source.is_none() || !matches!(
            self.snapshot.status,
            PlayerStatus::Ready | PlayerStatus::Playing | PlayerStatus::Paused
        ) { return; }
        self.snapshot.duration_ms = duration_ms;
        self.snapshot.position_ms = duration_ms.map_or(position_ms, |duration| position_ms.min(duration));
    }

    pub fn report_paused(&mut self) {
        if self.source.is_some() && self.snapshot.status == PlayerStatus::Playing {
            self.snapshot.status = PlayerStatus::Paused;
        }
    }

    pub fn report_playing(&mut self) {
        if self.source.is_some() && matches!(
            self.snapshot.status,
            PlayerStatus::Ready | PlayerStatus::Playing | PlayerStatus::Paused
        ) {
            self.snapshot.status = PlayerStatus::Playing;
        }
    }

    pub fn report_finished(&mut self) {
        if self.source.is_none() || !matches!(
            self.snapshot.status,
            PlayerStatus::Ready | PlayerStatus::Playing | PlayerStatus::Paused
        ) { return; }
        self.snapshot.status = PlayerStatus::Stopped;
        if let Some(duration) = self.snapshot.duration_ms {
            self.snapshot.position_ms = duration;
        }
    }

    pub fn report_error(&mut self, message: impl Into<String>) {
        if self.source.is_none() || !matches!(
            self.snapshot.status,
            PlayerStatus::Ready | PlayerStatus::Playing | PlayerStatus::Paused
        ) { return; }
        self.snapshot.status = PlayerStatus::Failed;
        self.snapshot.error = Some(message.into());
    }

    pub fn stop(&mut self) -> Option<PlayerCommand> {
        if self.source.is_none() || self.snapshot.status == PlayerStatus::Empty {
            return None;
        }
        self.snapshot.status = PlayerStatus::Stopped;
        self.snapshot.position_ms = 0;
        Some(PlayerCommand::Stop)
    }

    pub fn clear(&mut self) {
        self.source = None;
        self.snapshot = PlayerSnapshot::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> PlaybackSource {
        PlaybackSource {
            source_id: "smb-nas".into(),
            uri: "http://127.0.0.1:9000/media".into(),
            display_name: "film.mkv".into(),
            size_bytes: Some(1024),
            mime_type: Some("video/x-matroska".into()),
        }
    }

    #[test]
    fn load_and_transport_controls_are_backend_neutral() {
        let mut player = PlayerController::new();
        assert_eq!(player.load(source()), PlayerCommand::Load(source()));
        assert_eq!(player.snapshot().status, PlayerStatus::Ready);
        assert_eq!(player.play(), Some(PlayerCommand::Play));
        assert_eq!(player.pause(), Some(PlayerCommand::Pause));
        assert_eq!(player.play(), Some(PlayerCommand::Play));
    }

    #[test]
    fn seek_is_clamped_to_the_reported_duration() {
        let mut player = PlayerController::new();
        player.load(source());
        player.report_position(100, Some(10_000));
        assert_eq!(player.seek_to(20_000), Some(PlayerCommand::SeekTo { position_ms: 10_000 }));
        assert_eq!(player.snapshot().position_ms, 10_000);
    }

    #[test]
    fn failed_or_empty_sessions_reject_transport_commands() {
        let mut player = PlayerController::new();
        assert_eq!(player.play(), None);
        assert_eq!(player.seek_to(100), None);
        player.load(source());
        player.report_error("decoder unavailable");
        assert_eq!(player.play(), None);
        assert_eq!(player.seek_to(100), None);
        assert_eq!(player.snapshot().error.as_deref(), Some("decoder unavailable"));
    }

    #[test]
    fn late_engine_reports_cannot_resurrect_a_cleared_session() {
        let mut player = PlayerController::new();
        player.load(source());
        player.report_position(5_000, Some(10_000));
        player.clear();

        player.report_position(9_000, Some(10_000));
        player.report_paused();
        player.report_playing();
        player.report_finished();
        player.report_error("late teardown error");

        assert_eq!(player.snapshot(), &PlayerSnapshot::default());
        assert!(player.source().is_none());
    }

    #[test]
    fn delayed_engine_reports_cannot_resurrect_stopped_or_failed_sessions() {
        let mut finished = PlayerController::new();
        finished.load(source());
        finished.report_position(8_000, Some(10_000));
        finished.report_finished();
        finished.report_position(2_000, Some(10_000));
        finished.report_playing();
        assert_eq!(finished.snapshot().status, PlayerStatus::Stopped);
        assert_eq!(finished.snapshot().position_ms, 10_000);

        let mut stopped = PlayerController::new();
        stopped.load(source());
        stopped.report_position(4_000, Some(10_000));
        assert_eq!(stopped.stop(), Some(PlayerCommand::Stop));
        stopped.report_position(9_000, Some(10_000));
        stopped.report_playing();
        stopped.report_finished();
        stopped.report_error("late stop teardown error");
        assert_eq!(stopped.snapshot().status, PlayerStatus::Stopped);
        assert_eq!(stopped.snapshot().position_ms, 0);
        assert_eq!(stopped.snapshot().error, None);

        let mut failed = PlayerController::new();
        failed.load(source());
        failed.report_error("decoder failed");
        failed.report_position(7_000, Some(10_000));
        failed.report_playing();
        failed.report_finished();
        failed.report_error("late replacement error");
        assert_eq!(failed.snapshot().status, PlayerStatus::Failed);
        assert_eq!(failed.snapshot().position_ms, 0);
        assert_eq!(failed.snapshot().error.as_deref(), Some("decoder failed"));
    }

    #[test]
    fn finish_stop_and_clear_have_deterministic_state() {
        let mut player = PlayerController::new();
        player.load(source());
        player.report_position(500, Some(1_000));
        player.report_finished();
        assert_eq!(player.snapshot().status, PlayerStatus::Stopped);
        assert_eq!(player.snapshot().position_ms, 1_000);
        assert_eq!(player.stop(), Some(PlayerCommand::Stop));
        player.clear();
        assert_eq!(player.snapshot(), &PlayerSnapshot::default());
        assert!(player.source().is_none());
    }
}


/// Core-contract bridge for saved playback progress.
pub mod progress;
