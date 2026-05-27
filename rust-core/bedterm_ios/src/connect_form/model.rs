//! Pure-logic connect-form draft + validation. Mirrors the user-facing
//! invariants in `ConnectionFormViewModel` (host non-empty, port in
//! `1..=65535`, username non-empty, password / key required when the
//! user touched the secret field in add mode). The actual persistence
//! still runs through Swift's `ConnectionFormViewModel.save` — Rust
//! ships the validated draft over the FFI as JSON.

/// Auth method picked in the segmented control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMode {
    Password,
    Key,
}

/// Snapshot of the form's current state. Mirrors the SwiftUI view-model
/// without dragging keychain types across the FFI seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectFormDraft {
    /// `None` for new hosts; `Some(UUID-string)` when editing.
    pub id: Option<String>,
    pub label: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_mode: AuthMode,
    /// Whether a password / private key already lives in the Keychain
    /// (edit mode only). Used to decide whether the user *must* type a
    /// fresh secret to save.
    pub password_set: bool,
    pub key_set: bool,
    /// Display label for the picked key (e.g. file name). UI-only.
    pub key_label: Option<String>,
    /// Did the user type into the password field since open?
    pub password_touched: bool,
    /// Did the user pick a key file since open?
    pub key_touched: bool,
}

impl ConnectFormDraft {
    /// Blank draft (used for the "add" path).
    pub fn new() -> Self {
        Self {
            id: None,
            label: String::new(),
            host: String::new(),
            port: String::from("22"),
            username: String::new(),
            auth_mode: AuthMode::Password,
            password_set: false,
            key_set: false,
            key_label: None,
            password_touched: false,
            key_touched: false,
        }
    }
}

impl Default for ConnectFormDraft {
    fn default() -> Self {
        Self::new()
    }
}

/// Reasons a draft is unsaveable. The discriminant order is part of the
/// FFI surface — Swift maps it to a localized error message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    HostRequired,
    UsernameRequired,
    PortInvalid,
    PasswordRequired,
    KeyRequired,
}

impl ValidationError {
    /// English copy matching the SwiftUI form's inline error messages.
    /// Routes through `String(localized:)` on the Swift side once
    /// surfaced — until then this is the canonical fallback.
    pub fn message(&self) -> &'static str {
        match self {
            ValidationError::HostRequired
            | ValidationError::UsernameRequired
            | ValidationError::PortInvalid => "Host, port and username are required.",
            ValidationError::PasswordRequired => "Password is required.",
            ValidationError::KeyRequired => "Please import a private key file.",
        }
    }
}

/// Strip surrounding whitespace and split a pasted `host:port` (or
/// `[ipv6]:port`) into the host portion. Matches
/// `ConnectionFormViewModel.normalizedHost`.
pub fn normalized_host(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(bracket) = rest.find(']') {
            let inside = &rest[..bracket];
            let after = &rest[bracket + 1..];
            if after.starts_with(':') {
                return inside.to_string();
            }
            return trimmed.to_string();
        }
    }
    let colon_count = trimmed.chars().filter(|&c| c == ':').count();
    if colon_count != 1 {
        return trimmed.to_string();
    }
    if let Some(idx) = trimmed.rfind(':') {
        let tail = &trimmed[idx + 1..];
        if tail.parse::<u16>().is_ok() {
            return trimmed[..idx].to_string();
        }
    }
    trimmed.to_string()
}

/// Pull a port out of either the `host:port` paste form or the port
/// field. Returns `None` when no valid integer is recoverable.
pub fn normalized_port(host_raw: &str, port_raw: &str) -> Option<u16> {
    let host_trimmed = host_raw.trim();
    if let Some(rest) = host_trimmed.strip_prefix('[') {
        if let Some(bracket) = rest.find(']') {
            let after = &rest[bracket + 1..];
            if let Some(tail) = after.strip_prefix(':') {
                if let Ok(n) = tail.parse::<u16>() {
                    return Some(n);
                }
            }
        }
    }
    let colon_count = host_trimmed.chars().filter(|&c| c == ':').count();
    if colon_count == 1 {
        if let Some(idx) = host_trimmed.rfind(':') {
            if let Ok(n) = host_trimmed[idx + 1..].parse::<u16>() {
                return Some(n);
            }
        }
    }
    port_raw.trim().parse::<u16>().ok()
}

impl ConnectFormDraft {
    /// True iff the Save button should be enabled. Mirrors `canSave`
    /// in the SwiftUI view-model. Tests + Swift bridge are the only
    /// consumers; the VC reads validation via [`Self::validate`].
    #[allow(dead_code)]
    pub fn can_save(&self) -> bool {
        self.validate().is_ok()
    }

    /// Returns `Ok(())` when persistable, `Err(reason)` otherwise.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if normalized_host(&self.host).is_empty() {
            return Err(ValidationError::HostRequired);
        }
        if self.username.trim().is_empty() {
            return Err(ValidationError::UsernameRequired);
        }
        match normalized_port(&self.host, &self.port) {
            Some(p) if (1..=65535).contains(&p) => {}
            _ => return Err(ValidationError::PortInvalid),
        }
        let editing = self.id.is_some();
        match self.auth_mode {
            AuthMode::Password => {
                let needs_fresh = !editing || !self.password_set || self.password_touched;
                if needs_fresh && !self.password_touched {
                    // In edit mode with a stored password, untouched is fine.
                    if editing && self.password_set {
                        return Ok(());
                    }
                    return Err(ValidationError::PasswordRequired);
                }
                Ok(())
            }
            AuthMode::Key => {
                let has_key = self.key_set || self.key_touched;
                if !has_key {
                    return Err(ValidationError::KeyRequired);
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_draft() -> ConnectFormDraft {
        ConnectFormDraft {
            host: "10.0.0.5".to_string(),
            port: "22".to_string(),
            username: "yi".to_string(),
            password_touched: true,
            ..ConnectFormDraft::new()
        }
    }

    #[test]
    fn blank_host_rejected() {
        let d = ConnectFormDraft::new();
        assert_eq!(d.validate(), Err(ValidationError::HostRequired));
    }

    #[test]
    fn blank_username_rejected() {
        let d = ConnectFormDraft {
            host: "10.0.0.5".into(),
            ..ConnectFormDraft::new()
        };
        assert_eq!(d.validate(), Err(ValidationError::UsernameRequired));
    }

    #[test]
    fn port_out_of_range_rejected() {
        let mut d = add_draft();
        d.port = "70000".to_string();
        assert_eq!(d.validate(), Err(ValidationError::PortInvalid));
    }

    #[test]
    fn bad_port_string_rejected() {
        let mut d = add_draft();
        d.port = "abc".to_string();
        assert_eq!(d.validate(), Err(ValidationError::PortInvalid));
    }

    #[test]
    fn zero_port_rejected() {
        let mut d = add_draft();
        d.port = "0".to_string();
        assert_eq!(d.validate(), Err(ValidationError::PortInvalid));
    }

    #[test]
    fn password_required_in_add_mode_when_untouched() {
        let d = ConnectFormDraft {
            host: "10.0.0.5".into(),
            port: "22".into(),
            username: "yi".into(),
            password_touched: false,
            ..ConnectFormDraft::new()
        };
        assert_eq!(d.validate(), Err(ValidationError::PasswordRequired));
    }

    #[test]
    fn key_required_in_key_mode() {
        let d = ConnectFormDraft {
            host: "10.0.0.5".into(),
            port: "22".into(),
            username: "yi".into(),
            auth_mode: AuthMode::Key,
            ..ConnectFormDraft::new()
        };
        assert_eq!(d.validate(), Err(ValidationError::KeyRequired));
    }

    #[test]
    fn ipv6_bracketed_host_with_port_parses() {
        assert_eq!(normalized_host("[::1]:2222"), "::1");
        assert_eq!(normalized_port("[::1]:2222", "22"), Some(2222));
    }

    #[test]
    fn ipv6_bare_host_no_split() {
        // Multiple colons without brackets — treat the whole thing as host.
        assert_eq!(normalized_host("fe80::1"), "fe80::1");
    }

    #[test]
    fn pasted_host_port_split() {
        assert_eq!(normalized_host("10.0.0.5:2200"), "10.0.0.5");
        assert_eq!(normalized_port("10.0.0.5:2200", "22"), Some(2200));
    }

    #[test]
    fn edit_with_stored_password_saves_untouched() {
        let d = ConnectFormDraft {
            id: Some("UUID-X".into()),
            host: "10.0.0.5".into(),
            port: "22".into(),
            username: "yi".into(),
            password_set: true,
            password_touched: false,
            ..ConnectFormDraft::new()
        };
        assert!(d.can_save());
    }

    #[test]
    fn edit_with_stored_key_saves_untouched() {
        let d = ConnectFormDraft {
            id: Some("UUID-X".into()),
            host: "10.0.0.5".into(),
            port: "22".into(),
            username: "yi".into(),
            auth_mode: AuthMode::Key,
            key_set: true,
            key_touched: false,
            ..ConnectFormDraft::new()
        };
        assert!(d.can_save());
    }

    #[test]
    fn happy_path_add_password() {
        assert!(add_draft().can_save());
    }
}
