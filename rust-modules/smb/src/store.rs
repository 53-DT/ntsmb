//! Saved SMB connections that survive a cold reboot of the television.
//!
//! WHY THIS EXISTS. The debug `smb-smoke` trigger lives in `/tmp/<app id>/plxnative-smb`, and `/tmp`
//! is RAM on webOS: it survives an app reinstall but not a power cycle. A real app keeps its
//! connections on persistent storage. This module is that store.
//!
//! WHAT IT HOLDS. Only [`ConnectionProfile`] metadata (host, port, username, guest flag, share) and
//! the opaque, already-encrypted credential envelope that `ConnectionProfile::protect_password`
//! produced with the TV key manager. No plaintext password is ever serialised here, and nothing in
//! this file logs a credential.
//!
//! WHERE IT LIVES. This crate deliberately does not know the application's paths. The caller hands
//! in an ordered list of candidate file paths (see `docs/NT-SMB-NEXT-STEPS.md`, task 1) and the store
//! reads the first one that holds a valid file and writes to the first one that can be written.
//!
//! WRITES ARE ATOMIC: a temporary file beside the target is written, synced and renamed over it, so
//! a power cut can leave the old file or the new file but never half of one.

use crate::profile::ConnectionProfile;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const STORE_SCHEMA: u32 = 1;
/// More than this many saved servers is a mistake or abuse, not a household.
pub const MAX_SAVED: usize = 16;

/// One saved server.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SavedConnection {
    /// Stable identity, see [`connection_id`]. Contains no secret.
    pub id: String,
    /// What the list shows. Defaults to the host.
    pub name: String,
    pub profile: ConnectionProfile,
    /// The folder (relative to the share root) the person was last in. Empty means the share root.
    #[serde(default)]
    pub last_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SavedFile {
    schema: u32,
    #[serde(default)]
    last_used: Option<String>,
    #[serde(default)]
    connections: Vec<SavedConnection>,
}

impl SavedFile {
    fn empty() -> Self {
        Self { schema: STORE_SCHEMA, last_used: None, connections: Vec::new() }
    }
}

/// `host:port/username` (or `/guest`), lower-cased host. Two forms that name the same server and
/// account collapse into one saved entry.
pub fn connection_id(profile: &ConnectionProfile) -> String {
    let who = if profile.guest { "guest".to_owned() } else { profile.username.trim().to_owned() };
    format!("{}:{}/{}", profile.host.trim().to_ascii_lowercase(), profile.port, who)
}

/// The saved connections, in memory, with the files they came from and go to.
pub struct SavedStore {
    candidates: Vec<PathBuf>,
    data: SavedFile,
}

impl SavedStore {
    /// Load from the first candidate that holds a valid file. A missing file is an empty store.
    /// A file that exists but cannot be understood is moved aside (`.corrupt`) rather than
    /// silently overwritten, and the store starts empty.
    pub fn open(candidates: Vec<PathBuf>) -> Self {
        let mut data = SavedFile::empty();
        for path in &candidates {
            match fs::read_to_string(path) {
                Ok(text) => match Self::parse(&text) {
                    Some(parsed) => {
                        data = parsed;
                        break;
                    }
                    None => {
                        let mut aside = path.clone().into_os_string();
                        aside.push(".corrupt");
                        let _ = fs::rename(path, PathBuf::from(aside));
                    }
                },
                Err(_) => continue,
            }
        }
        Self { candidates, data }
    }

    fn parse(text: &str) -> Option<SavedFile> {
        let mut file: SavedFile = serde_json::from_str(text).ok()?;
        if file.schema != STORE_SCHEMA {
            return None;
        }
        // One bad entry must not cost the person the good ones.
        file.connections.retain(|c| c.profile.validate().is_ok());
        file.connections.truncate(MAX_SAVED);
        let last_is_known = match file.last_used.as_deref() {
            Some(last) => file.connections.iter().any(|c| c.id == last),
            None => true,
        };
        if !last_is_known {
            file.last_used = None;
        }
        Some(file)
    }

    pub fn connections(&self) -> &[SavedConnection] {
        &self.data.connections
    }

    pub fn get(&self, id: &str) -> Option<&SavedConnection> {
        self.data.connections.iter().find(|c| c.id == id)
    }

    /// The server to reconnect to at launch, if any.
    pub fn last_used(&self) -> Option<&SavedConnection> {
        let id = self.data.last_used.as_deref()?;
        self.get(id)
    }

    /// Add or replace the entry for this server and account, and make it the last used. Returns
    /// its id. The profile is validated first; a refused profile changes nothing.
    pub fn upsert(&mut self, profile: ConnectionProfile) -> Result<String, String> {
        profile.validate()?;
        let id = connection_id(&profile);
        if let Some(existing) = self.data.connections.iter_mut().find(|c| c.id == id) {
            // Keep the person's folder if they re-save the same server from the form.
            let keep_path = if existing.profile.share == profile.share { existing.last_path.clone() } else { String::new() };
            existing.profile = profile;
            existing.last_path = keep_path;
        } else {
            if self.data.connections.len() >= MAX_SAVED {
                return Err("Too many saved servers. Remove one first.".into());
            }
            let name = profile.host.clone();
            self.data.connections.push(SavedConnection { id: id.clone(), name, profile, last_path: String::new() });
        }
        self.data.last_used = Some(id.clone());
        Ok(id)
    }

    pub fn rename(&mut self, id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(c) = self.data.connections.iter_mut().find(|c| c.id == id) {
            c.name = name.chars().take(48).collect();
        }
    }

    pub fn mark_used(&mut self, id: &str) {
        if self.get(id).is_some() {
            self.data.last_used = Some(id.to_owned());
        }
    }

    pub fn set_last_path(&mut self, id: &str, path: &str) {
        if let Some(c) = self.data.connections.iter_mut().find(|c| c.id == id) {
            c.last_path = path.trim_matches('/').to_owned();
        }
    }

    /// Remember which share a saved server opens on, once the person has chosen one.
    pub fn set_share(&mut self, id: &str, share: Option<&str>) {
        if let Some(c) = self.data.connections.iter_mut().find(|c| c.id == id) {
            let share = share.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
            if c.profile.share != share {
                c.profile.share = share;
                c.last_path.clear();
            }
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.data.connections.retain(|c| c.id != id);
        if self.data.last_used.as_deref() == Some(id) {
            self.data.last_used = None;
        }
    }

    /// Write to the first candidate that can be written. The error never names a path or a
    /// credential, because it can reach the screen and the log.
    pub fn save(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.data).map_err(|_| "Could not encode saved servers".to_string())?;
        for path in &self.candidates {
            if write_atomic(path, text.as_bytes()).is_ok() {
                return Ok(());
            }
        }
        Err("Could not save to the television's storage".into())
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut tmp = path.to_path_buf().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::CredentialEnvelope;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ntsmb-store-test-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sealed(profile: &mut ConnectionProfile) {
        profile.credential = Some(CredentialEnvelope {
            backend: "keymanager3".into(),
            key: "ntsmb.credentials.v1".into(),
            iv: "opaque-iv".into(),
            data: "opaque-ciphertext".into(),
        });
    }

    #[test]
    fn a_saved_server_survives_a_restart_and_holds_no_plaintext_password() {
        let dir = scratch("roundtrip");
        let file = dir.join("smb-connections.json");
        let mut store = SavedStore::open(vec![file.clone()]);
        let mut profile = ConnectionProfile::new("Nas.Local", 445, "alice", false).unwrap();
        profile.share = Some("Shared".into());
        sealed(&mut profile);
        let id = store.upsert(profile).unwrap();
        assert_eq!(id, "nas.local:445/alice");
        store.save().unwrap();

        let text = fs::read_to_string(&file).unwrap();
        assert!(!text.to_lowercase().contains("\"password\""));

        let reopened = SavedStore::open(vec![file]);
        assert_eq!(reopened.connections().len(), 1);
        assert_eq!(reopened.last_used().unwrap().id, id);
        assert_eq!(reopened.last_used().unwrap().profile.share.as_deref(), Some("Shared"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_the_same_server_twice_replaces_it_and_keeps_the_folder() {
        let dir = scratch("replace");
        let mut store = SavedStore::open(vec![dir.join("a.json")]);
        let id = store.upsert(ConnectionProfile::new("nas", 445, "alice", false).unwrap()).unwrap();
        store.set_last_path(&id, "/Media/Shows/");
        let again = store.upsert(ConnectionProfile::new("NAS", 445, "alice", false).unwrap()).unwrap();
        assert_eq!(id, again);
        assert_eq!(store.connections().len(), 1);
        assert_eq!(store.get(&id).unwrap().last_path, "Media/Shows");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_file_is_an_empty_store_and_a_corrupt_one_is_set_aside() {
        let dir = scratch("corrupt");
        let file = dir.join("c.json");
        assert!(SavedStore::open(vec![file.clone()]).connections().is_empty());
        fs::write(&file, "{ not json").unwrap();
        let store = SavedStore::open(vec![file.clone()]);
        assert!(store.connections().is_empty());
        assert!(!file.exists());
        assert!(dir.join("c.json.corrupt").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn one_invalid_entry_does_not_cost_the_valid_ones() {
        let dir = scratch("partial");
        let file = dir.join("p.json");
        let good = ConnectionProfile::new("nas", 445, "alice", false).unwrap();
        let mut bad = good.clone();
        bad.port = 0;
        let data = SavedFile {
            schema: STORE_SCHEMA,
            last_used: None,
            connections: vec![
                SavedConnection { id: "bad".into(), name: "bad".into(), profile: bad, last_path: String::new() },
                SavedConnection { id: connection_id(&good), name: "nas".into(), profile: good, last_path: String::new() },
            ],
        };
        fs::write(&file, serde_json::to_string(&data).unwrap()).unwrap();
        let store = SavedStore::open(vec![file]);
        assert_eq!(store.connections().len(), 1);
        assert_eq!(store.connections()[0].name, "nas");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn save_falls_through_to_the_next_candidate_and_remove_forgets_last_used() {
        let dir = scratch("fallthrough");
        // A path under a regular file can never be created.
        let blocker = dir.join("blocker");
        fs::write(&blocker, "x").unwrap();
        let unwritable = blocker.join("nested").join("s.json");
        let good = dir.join("ok").join("s.json");
        let mut store = SavedStore::open(vec![unwritable, good.clone()]);
        let id = store.upsert(ConnectionProfile::new("nas", 445, "", true).unwrap()).unwrap();
        store.save().unwrap();
        assert!(good.exists());
        store.remove(&id);
        assert!(store.last_used().is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_full_store_refuses_a_seventeenth_server() {
        let dir = scratch("full");
        let mut store = SavedStore::open(vec![dir.join("f.json")]);
        for i in 0..MAX_SAVED {
            store.upsert(ConnectionProfile::new(format!("nas{i}"), 445, "u", false).unwrap()).unwrap();
        }
        assert!(store.upsert(ConnectionProfile::new("one-too-many", 445, "u", false).unwrap()).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}