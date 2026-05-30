#![allow(dead_code)]
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

/// Mode the form was opened in. Mirrors
/// `ConnectionFormViewModel.Mode` in the Swift layer.
///
/// - `Add` — creating a new host entry.
/// - `Edit(uuid)` — editing an existing entry identified by its UUID
///   string. The Rust state machine (`ConnectFormVM`) retains additional
///   edit-time state (original auth mode, cached secrets) on its own
///   `Mode` type; this is a lightweight projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditMode {
    Add,
    Edit(String),
}

/// Outcome of the connect-form flow, reported back to the hosts screen.
/// Mirrors `ConnectionFormOutcome` in the Swift layer.
///
/// - `Saved(id)` — user saved the entry.
/// - `SavedAndConnect(id)` — user saved and wants to connect immediately.
/// - `Cancelled` — user dismissed the form without saving.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionFormOutcome {
    Saved(String),
    SavedAndConnect(String),
    Cancelled,
}

/// Snapshot of the form's current state. Mirrors the SwiftUI view-model
/// without dragging keychain types across the FFI seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectFormDraft {
    pub id: Option<String>,
    pub label: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_mode: AuthMode,
    pub password_set: bool,
    pub password_touched: bool,
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
            password_touched: false,
        }
    }
}

impl Default for ConnectFormDraft {
    fn default() -> Self {
        Self::new()
    }
}

/// Reasons a draft is unsaveable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    HostRequired,
    UsernameRequired,
    PortInvalid,
    PasswordRequired,
    KeyComingSoon,
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
            ValidationError::KeyComingSoon => "Key authentication coming soon.",
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
            AuthMode::Key => Err(ValidationError::KeyComingSoon),
        }
    }
}
