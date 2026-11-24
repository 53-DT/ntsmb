//! Persistable NT-SMB connection profile metadata.
//!
//! Passwords are never part of this profile as plaintext. The application seals the password
//! with the platform credential store and places only the opaque envelope in this structure.
//! This crate intentionally does not choose a filesystem path or a platform key manager.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const PROFILE_SCHEMA: u32 = 1;
pub const DEFAULT_PORT: u16 = 445;

/// Opaque authenticated-encryption envelope produced by the platform key manager.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CredentialEnvelope {
    pub backend: String,
    pub key: String,
    pub iv: String,
    pub data: String,
}

impl fmt::Debug for CredentialEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialEnvelope { <redacted> }")
    }
}

/// Saved connection settings. The password is represented only by a sealed envelope.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionProfile {
    pub schema: u32,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub guest: bool,
    pub share: Option<String>,
    pub credential: Option<CredentialEnvelope>,
}

impl fmt::Debug for ConnectionProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionProfile")
            .field("schema", &self.schema)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("guest", &self.guest)
            .field("share", &self.share)
            .field("credential", &self.credential.as_ref().map(|_| "<sealed>"))
            .finish()
    }
}

impl ConnectionProfile {
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>, guest: bool) -> Result<Self, String> {
        let profile = Self {
            schema: PROFILE_SCHEMA,
            host: host.into().trim().to_owned(),
            port,
            username: username.into(),
            guest,
            share: None,
            credential: None,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != PROFILE_SCHEMA {
            return Err("Unsupported saved SMB profile version".into());
        }
        if self.host.is_empty()
            || self.host.contains("://")
            || self.host.contains('/')
            || self.host.contains('\\')
            || self.host.chars().any(char::is_whitespace)
            || self.host.contains(':')
        {
            return Err("Enter a hostname or IP address without a port".into());
        }
        if self.port == 0 {
            return Err("SMB port must be between 1 and 65535".into());
        }
        if !self.guest && self.username.trim().is_empty() {
            return Err("Enter an SMB username or enable Guest".into());
        }
        if self.guest && self.credential.is_some() {
            return Err("Guest profiles must not contain a password".into());
        }
        if self.share.as_deref().is_some_and(|share| {
            share.trim().is_empty() || share.contains('/') || share.contains('\\')
        }) {
            return Err("Invalid saved SMB share".into());
        }
        Ok(())
    }

    /// Serialize only validated metadata and the opaque encrypted credential envelope.
    pub fn encode(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| "Could not encode SMB profile".into())
    }

    /// Decode a saved profile without ever interpreting the credential envelope as plaintext.
    pub fn decode(encoded: &str) -> Result<Self, String> {
        let profile: Self = serde_json::from_str(encoded)
            .map_err(|_| "Saved SMB profile is invalid".to_string())?;
        profile.validate()?;
        Ok(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_port_and_host_normalization_are_explicit() {
        let profile = ConnectionProfile::new(" nas.local ", DEFAULT_PORT, "alice", false).unwrap();
        assert_eq!(profile.host, "nas.local");
        assert_eq!(profile.port, 445);
    }

    #[test]
    fn profile_round_trip_keeps_only_an_opaque_credential_envelope() {
        let mut profile = ConnectionProfile::new("nas.local", 445, "alice", false).unwrap();
        profile.credential = Some(CredentialEnvelope {
            backend: "keymanager3".into(),
            key: "ntsmb.credentials.v1".into(),
            iv: "opaque-iv".into(),
            data: "opaque-ciphertext".into(),
        });
        let encoded = profile.encode().unwrap();
        assert!(!encoded.contains("password"));
        assert!(!encoded.contains("alice-secret"));
        assert_eq!(ConnectionProfile::decode(&encoded).unwrap(), profile);
        assert!(!format!("{profile:?}").contains("opaque-ciphertext"));
    }

    #[test]
    fn guest_profile_never_accepts_a_password_envelope() {
        let mut profile = ConnectionProfile::new("nas.local", 445, "", true).unwrap();
        profile.credential = Some(CredentialEnvelope {
            backend: "keymanager3".into(),
            key: "k".into(),
            iv: "i".into(),
            data: "d".into(),
        });
        assert!(profile.validate().is_err());
    }

    #[test]
    fn host_and_port_are_validated_independently() {
        assert!(ConnectionProfile::new("nas.local:1445", 445, "alice", false).is_err());
        assert!(ConnectionProfile::new("nas.local", 0, "alice", false).is_err());
        assert!(ConnectionProfile::new("nas.local", 445, "", false).is_err());
    }
}
