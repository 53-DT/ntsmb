//! Backend- and UI-neutral domain types and repository contracts for NTSMB.
//!
//! This crate deliberately has no SMB, Jellyfin, FinNative, webOS, player-engine,
//! rendering, or application dependencies. Backend adapters translate their native
//! values into these contracts; UI and playback code consume only these types.

use serde::{Deserialize, Serialize};

/// Stable identifier for an item within its source.
pub type MediaId = String;

/// A single browsable media entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: MediaId,
    pub name: String,
    /// Path relative to the selected source root, using forward slashes.
    pub path: String,
    pub size_bytes: Option<u64>,
    pub is_directory: bool,
    pub media_type: MediaType,
}

/// Broad media classification; intentionally independent of any backend or codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    Folder,
    Video,
    Audio,
    Image,
    Other,
}

impl Default for MediaType {
    fn default() -> Self { Self::Other }
}

/// A folder listing returned by a media source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFolder {
    pub source_id: String,
    /// Empty string represents the source root.
    pub path: String,
    pub parent_path: Option<String>,
    pub items: Vec<MediaItem>,
}

/// A backend-independent playable resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackSource {
    /// Opaque source identifier. It must not contain credentials.
    pub source_id: String,
    /// A local or otherwise player-consumable URI. Credentials must never be embedded.
    pub uri: String,
    pub display_name: String,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
}

/// A backend-owned resource whose lifetime must cover the active playback session.
///
/// The player retains this opaque lease without depending on the source implementation. Dropping
/// it releases backend resources (for example, an SMB Range server and its authenticated session).
/// Platform repair and native decoder resources are deliberately not represented by this trait.
pub trait PlaybackLease: Send + Sync {}

/// User-managed connection/profile metadata. Secret material is deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub name: String,
    pub source_id: Option<String>,
    pub username: Option<String>,
    pub is_guest: bool,
}

/// General application preferences, with no UI toolkit or platform-specific values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppPreferences {
    pub autoplay_next: bool,
    pub show_hidden_files: bool,
    pub preferred_audio_language: Option<String>,
    pub preferred_subtitle_language: Option<String>,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            autoplay_next: true,
            show_hidden_files: false,
            preferred_audio_language: None,
            preferred_subtitle_language: None,
        }
    }
}

/// Persistable playback progress for one media item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackState {
    pub media_id: MediaId,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub completed: bool,
}

/// Backend adapter contract for discovering sources, browsing folders and opening media.
pub trait MediaSourceRepository: Send + Sync {
    fn list_sources(&self) -> Result<Vec<MediaItem>, String>;
    fn list_folder(&self, source_id: &str, path: &str) -> Result<MediaFolder, String>;
    fn open_playback(&self, source_id: &str, path: &str) -> Result<PlaybackSource, String>;
}

/// Persistence contract for local user profiles.
pub trait ProfileRepository: Send + Sync {
    fn load_profiles(&self) -> Result<Vec<UserProfile>, String>;
    fn save_profile(&self, profile: &UserProfile) -> Result<(), String>;
    fn delete_profile(&self, profile_id: &str) -> Result<(), String>;
}

/// Persistence contract for resume/watch progress.
pub trait PlaybackStateRepository: Send + Sync {
    fn load_state(&self, media_id: &str) -> Result<Option<PlaybackState>, String>;
    fn save_state(&self, state: &PlaybackState) -> Result<(), String>;
    fn delete_state(&self, media_id: &str) -> Result<(), String>;
}


/// Platform-neutral result of the boot-time native-video capability probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeVideoVerdict {
    /// The platform probed and native video is available.
    Available,
    /// The platform probed and native video is blocked for this reason.
    Blocked(&'static str),
    /// The platform has no applicable native-video preflight.
    NotApplicable,
}

/// Failure categories returned by an explicitly requested platform repair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeVideoRepairFailure {
    StartFailed,
    HbcUnavailable,
    NotRoot,
    CommandFailed,
    Timeout,
    Unreadable,
    Unsupported,
}

/// Process-lifetime state of the one permitted native-video repair attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeVideoRepairState {
    Idle,
    Running,
    Repaired,
    Failed(NativeVideoRepairFailure),
}

/// Neutral port through which the player can query a platform's native-video verdict and request
/// its user-authorized repair. The platform owns the implementation; player/core owns policy only.
pub trait NativeVideoRepairPort: Send + Sync + 'static {
    fn verdict(&self) -> NativeVideoVerdict;
    fn repair(&self) -> Result<(), NativeVideoRepairFailure>;
}
