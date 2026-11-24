//! Input and validation model for the NT-SMB connection screen.
//!
//! The UI owns these editable strings. Validation produces a request whose password is
//! deliberately excluded from Debug and from the persisted profile. UI integration can consume
//! this without placing credentials into app navigation or diagnostic state.

use crate::profile::{ConnectionProfile, DEFAULT_PORT};
use std::fmt;

/// Editable fields presented on the SMB welcome/connect screen.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ConnectForm {
    pub host: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub guest: bool,
    pub save_connection: bool,
}

impl fmt::Debug for ConnectForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectForm")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("guest", &self.guest)
            .field("save_connection", &self.save_connection)
            .finish()
    }
}

impl ConnectForm {
    /// Form with the standard SMB port prefilled.
    pub fn new() -> Self {
        Self { port: DEFAULT_PORT.to_string(), ..Self::default() }
    }

    /// Parse/validate fields and create a transient connection request.
    pub fn validate(&self) -> Result<ConnectRequest, String> {
        let host = self.host.trim();
        if host.is_empty() {
            return Err("Enter the SMB server name or IP address".into());
        }
        let port = self.port.trim().parse::<u16>()
            .map_err(|_| "Enter a valid port number (1–65535)".to_string())?;
        if port == 0 {
            return Err("Port must be between 1 and 65535".into());
        }
        let profile = ConnectionProfile::new(
            host,
            port,
            if self.guest { "" } else { self.username.trim() },
            self.guest,
        )?;
        if !self.guest && self.password.is_empty() {
            return Err("Enter the SMB password, or enable Guest".into());
        }
        Ok(ConnectRequest { profile, password: if self.guest { String::new() } else { self.password.clone() } })
    }

    /// Produce a profile ready for persistence, protecting the password first. Saving is refused
    /// when the platform secure store cannot durably protect it; no plaintext fallback exists.
    pub fn saved_profile(&self) -> Result<ConnectionProfile, String> {
        let mut request = self.validate()?;
        if !self.save_connection {
            return Err("Save connection is not enabled".into());
        }
        if !request.profile.guest {
            request.profile.protect_password(&request.password)?;
        }
        Ok(request.profile)
    }
}

/// Transient connect parameters. Password intentionally never enters profile serialization.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectRequest {
    pub profile: ConnectionProfile,
    password: String,
}

impl ConnectRequest {
    pub fn password(&self) -> &str { &self.password }
}

impl fmt::Debug for ConnectRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectRequest")
            .field("profile", &self.profile)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_port_is_prefilled_and_guest_needs_no_username_or_password() {
        let mut form = ConnectForm::new();
        assert_eq!(form.port, "445");
        form.host = "nas.local".into();
        form.guest = true;
        let req = form.validate().unwrap();
        assert_eq!(req.profile.host, "nas.local");
        assert_eq!(req.profile.port, 445);
        assert!(req.profile.guest);
        assert_eq!(req.password(), "");
    }

    #[test]
    fn authenticated_form_requires_username_and_password() {
        let mut form = ConnectForm::new();
        form.host = "nas.local".into();
        assert!(form.validate().is_err());
        form.username = "viewer".into();
        assert!(form.validate().is_err());
        form.password = "secret".into();
        let request = form.validate().unwrap();
        assert_eq!(request.profile.username, "viewer");
        assert_eq!(request.password(), "secret");
        assert!(!format!("{form:?}").contains("secret"));
        assert!(!format!("{request:?}").contains("secret"));
    }

    #[test]
    fn host_and_port_validation_is_friendly_and_strict() {
        let mut form = ConnectForm::new();
        assert!(form.validate().unwrap_err().contains("server"));
        form.host = "nas.local".into();
        form.username = "viewer".into();
        form.password = "pw".into();
        form.port = "0".into();
        assert!(form.validate().unwrap_err().contains("Port"));
        form.port = "65536".into();
        assert!(form.validate().unwrap_err().contains("valid port"));
        form.port = "1445".into();
        assert_eq!(form.validate().unwrap().profile.port, 1445);
    }

    #[test]
    fn save_profile_refuses_unchecked_and_never_plaintext_fallback() {
        let mut form = ConnectForm::new();
        form.host = "nas.local".into();
        form.username = "viewer".into();
        form.password = "secret".into();
        assert!(form.saved_profile().is_err());
        form.save_connection = true;
        let result = form.saved_profile();
        if plx_os::tv::secure::mode() == plx_os::tv::secure::StoreMode::MemoryOnly {
            assert!(result.is_err());
        } else {
            let profile = result.unwrap();
            assert!(profile.credential.is_some());
            assert!(!profile.encode().unwrap().contains("secret"));
        }
    }
}
