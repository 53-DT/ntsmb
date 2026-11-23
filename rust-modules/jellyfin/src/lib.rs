//! plx_jellyfin: the Jellyfin layer of FinNative (see [`jellyfin`]).
//!
//! It is the `jellyfin` layer of `ci/module-layers.ini`: it uses `base`, `machine`, `os`,
//! `net` and `plex` (for the common item model) and nothing above them.

pub mod jellyfin;

/// Direct SMB2/3 share access used by the native SMB browser.
pub use plx_smb as smb;
