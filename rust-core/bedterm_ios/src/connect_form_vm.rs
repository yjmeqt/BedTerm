//! Pure state machine for the connect-form (formerly Swift
//! `ConnectionFormViewModel`). Owns every form field, dirty tracking,
//! validation rules, and the secret-preservation logic for edit mode.
//!
//! No UIKit imports — this module is host-testable. The FFI singleton
//! lives in [`crate::ffi::connect_form_vm`] (iOS-gated).
//!
//! Mirrors the pattern used by [`crate::hosts_vm`]: the Swift layer is a
//! thin `@Observable` proxy that mirrors state on every mutation so
//! existing observation paths keep firing. Persistence (writing the
//! resolved `SavedHost` blob to the Keychain) stays in Swift because
//! `HostCredential` is Codable on the Swift side; Rust resolves all the
//! fields + secrets and hands back a JSON blob with everything Swift
//! needs to materialise a `SavedHost` and call `HostsStore.save`.

use crate::connect_form::model::{normalized_host, normalized_port};
use crate::l10n::t;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Auth kind picked in the segmented control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthKind {
    Password,
    PrivateKey,
}

/// Mode the form was opened in. In edit mode the VM also retains the
/// existing entry's secrets so it can preserve them when the user
/// doesn't touch the secret fields.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Add,
    Edit {
        id: String,
        original_auth: AuthKind,
        /// Cached existing password (only when original_auth = Password).
        existing_password: Option<String>,
        /// Cached existing private key bytes (only when original_auth = PrivateKey).
        existing_private_key: Option<Vec<u8>>,
        /// Cached existing passphrase (only when original_auth = PrivateKey).
        existing_passphrase: Option<String>,
    },
}

/// Resolved-fields output of a successful save attempt. Swift consumes
/// the JSON form (`SaveOutcome`'s serde shape) to build a `SavedHost`
/// and write it through `HostsStore`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveOutcome {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(rename = "authIsKey")]
    pub auth_is_key: bool,
    /// Resolved password — only populated when auth_is_key is false.
    pub password: Option<String>,
    /// Resolved private-key bytes — only populated when auth_is_key is true.
    #[serde(rename = "privateKeyBase64")]
    pub private_key_base64: Option<String>,
    /// Resolved passphrase — only populated when auth_is_key is true.
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectFormVM {
    pub mode: Mode,
    pub label: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub is_using_key: bool,
    pub password: String,
    pub private_key: Vec<u8>,
    pub passphrase: String,
    pub password_touched: bool,
    pub private_key_touched: bool,
    pub passphrase_touched: bool,
    pub error_message: Option<String>,
}

impl Default for ConnectFormVM {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectFormVM {
    pub const fn new() -> Self {
        Self {
            mode: Mode::Add,
            label: String::new(),
            host: String::new(),
            port: String::new(),
            username: String::new(),
            is_using_key: false,
            password: String::new(),
            private_key: Vec::new(),
            passphrase: String::new(),
            password_touched: false,
            private_key_touched: false,
            passphrase_touched: false,
            error_message: None,
        }
    }

    /// Reset to a blank Add-mode draft (default port 22).
    pub fn reset_to_add(&mut self) {
        *self = Self::new();
        self.port = String::from("22");
    }

    /// Prefill the form for an Edit-mode session. Captures the existing
    /// secrets so untouched secret fields preserve them on save.
    #[allow(clippy::too_many_arguments)]
    pub fn prefill_edit(
        &mut self,
        id: String,
        label: String,
        host: String,
        port: u16,
        username: String,
        auth_is_key: bool,
        existing_password: Option<String>,
        existing_private_key: Option<Vec<u8>>,
        existing_passphrase: Option<String>,
    ) {
        let original_auth = if auth_is_key {
            AuthKind::PrivateKey
        } else {
            AuthKind::Password
        };
        self.mode = Mode::Edit {
            id,
            original_auth,
            existing_password: existing_password.clone(),
            existing_private_key: existing_private_key.clone(),
            existing_passphrase: existing_passphrase.clone(),
        };
        self.label = label;
        self.host = host;
        self.port = port.to_string();
        self.username = username;
        self.is_using_key = auth_is_key;
        // Don't mirror the actual secret values into the editable fields
        // — that would leak the Keychain blob into the visible UI. We
        // hold them on `mode` and splice them in on save when the user
        // didn't touch the field.
        self.password.clear();
        self.private_key.clear();
        self.passphrase.clear();
        self.password_touched = false;
        self.private_key_touched = false;
        self.passphrase_touched = false;
        self.error_message = None;
    }

    // ─── Setters ────────────────────────────────────────────────────

    pub fn set_label(&mut self, value: String) {
        self.label = value;
    }
    pub fn set_host(&mut self, value: String) {
        self.host = value;
    }
    pub fn set_port_text(&mut self, value: String) {
        self.port = value;
    }
    pub fn set_username(&mut self, value: String) {
        self.username = value;
    }
    pub fn set_password(&mut self, value: String) {
        self.password = value;
        self.password_touched = true;
    }
    pub fn set_passphrase(&mut self, value: String) {
        self.passphrase = value;
        self.passphrase_touched = true;
    }
    pub fn set_private_key_bytes(&mut self, bytes: Vec<u8>) {
        self.private_key = bytes;
        self.private_key_touched = true;
    }
    pub fn set_using_key(&mut self, value: bool) {
        self.is_using_key = value;
    }

    pub fn has_password(&self) -> bool {
        match &self.mode {
            Mode::Add => !self.password.is_empty(),
            Mode::Edit {
                original_auth,
                existing_password,
                ..
            } => {
                if self.password_touched {
                    !self.password.is_empty()
                } else {
                    *original_auth == AuthKind::Password && existing_password.is_some()
                }
            }
        }
    }

    pub fn has_private_key(&self) -> bool {
        match &self.mode {
            Mode::Add => !self.private_key.is_empty(),
            Mode::Edit {
                original_auth,
                existing_private_key,
                ..
            } => {
                if self.private_key_touched {
                    !self.private_key.is_empty()
                } else {
                    *original_auth == AuthKind::PrivateKey && existing_private_key.is_some()
                }
            }
        }
    }

    pub fn has_passphrase(&self) -> bool {
        match &self.mode {
            Mode::Add => !self.passphrase.is_empty(),
            Mode::Edit {
                original_auth,
                existing_passphrase,
                ..
            } => {
                if self.passphrase_touched {
                    !self.passphrase.is_empty()
                } else {
                    *original_auth == AuthKind::PrivateKey && existing_passphrase.is_some()
                }
            }
        }
    }

    /// True iff the secret field for the current auth-mode needs a
    /// fresh value (because we're adding, swapped auth-modes, or the
    /// user touched the field).
    fn requires_fresh_secret(&self) -> bool {
        let Mode::Edit { original_auth, .. } = &self.mode else {
            return true;
        };
        let current_auth = if self.is_using_key {
            AuthKind::PrivateKey
        } else {
            AuthKind::Password
        };
        if current_auth != *original_auth {
            return true;
        }
        match current_auth {
            AuthKind::Password => self.password_touched,
            AuthKind::PrivateKey => self.private_key_touched,
        }
    }

    /// Re-check the form. Returns `None` when persistable, else a
    /// localized message that Swift can pull via the error_message FFI.
    pub fn validate(&self) -> Option<String> {
        let host = normalized_host(&self.host);
        let user = self.username.trim().to_string();
        if host.is_empty() || user.is_empty() {
            return Some(t("Host, port and username are required."));
        }
        let port_opt = normalized_port(&self.host, &self.port);
        match port_opt {
            Some(p) if (1..=65535).contains(&p) => {}
            _ => return Some(t("Port must be between 1 and 65535.")),
        }
        if self.is_using_key {
            // Need a key: either an untouched stored one or a freshly picked one.
            if self.requires_fresh_secret() {
                if self.private_key.is_empty() {
                    return Some(t("Please import a private key file."));
                }
            } else if !matches!(
                &self.mode,
                Mode::Edit {
                    existing_private_key: Some(_),
                    original_auth: AuthKind::PrivateKey,
                    ..
                }
            ) {
                return Some(t("Please import a private key file."));
            }
        } else {
            // Need a password: same logic.
            if self.requires_fresh_secret() {
                if self.password.is_empty() {
                    return Some(t("Host, port and username are required."));
                }
            } else if !matches!(
                &self.mode,
                Mode::Edit {
                    existing_password: Some(_),
                    original_auth: AuthKind::Password,
                    ..
                }
            ) {
                return Some(t("Host, port and username are required."));
            }
        }
        None
    }

    /// Convenience for the Save button's disabled state. Mirrors
    /// `canSave` in the SwiftUI view-model.
    pub fn can_save(&self) -> bool {
        self.validate().is_none()
    }

    /// Validate + build a [`SaveOutcome`]. On failure, sets
    /// `error_message` and returns `Err(message)`. On success, clears
    /// `error_message` and returns the resolved fields Swift needs to
    /// build + persist a `SavedHost`. The VM is *not* mutated past the
    /// error_message clear — Swift owns the persistence side effect.
    pub fn try_save(&mut self) -> Result<SaveOutcome, String> {
        if let Some(err) = self.validate() {
            self.error_message = Some(err.clone());
            return Err(err);
        }
        self.error_message = None;
        let host = normalized_host(&self.host);
        let port = normalized_port(&self.host, &self.port)
            .ok_or_else(|| t("Port must be between 1 and 65535."))?;
        let user = self.username.trim().to_string();
        let label = self.label.trim().to_string();
        let id = match &self.mode {
            Mode::Add => uuid_v4(),
            Mode::Edit { id, .. } => id.clone(),
        };
        let auth_is_key = self.is_using_key;
        let (password, private_key_bytes, passphrase) = self.resolve_secrets();
        let private_key_base64 = private_key_bytes.as_ref().map(|b| base64_encode(b));

        Ok(SaveOutcome {
            id,
            label,
            host,
            port,
            username: user,
            auth_is_key,
            password: if auth_is_key { None } else { password },
            private_key_base64: if auth_is_key {
                private_key_base64
            } else {
                None
            },
            passphrase: if auth_is_key { passphrase } else { None },
        })
    }

    /// Returns `(password_or_none, private_key_or_none, passphrase_or_none)`
    /// honouring the dirty-flag logic.
    fn resolve_secrets(&self) -> (Option<String>, Option<Vec<u8>>, Option<String>) {
        let preserve_password = matches!(
            &self.mode,
            Mode::Edit {
                original_auth: AuthKind::Password,
                existing_password: Some(_),
                ..
            }
        );
        let preserve_key = matches!(
            &self.mode,
            Mode::Edit {
                original_auth: AuthKind::PrivateKey,
                existing_private_key: Some(_),
                ..
            }
        );
        let preserve_pass = matches!(
            &self.mode,
            Mode::Edit {
                original_auth: AuthKind::PrivateKey,
                existing_passphrase: Some(_),
                ..
            }
        );
        let current_is_key = self.is_using_key;
        let switched_auth = matches!(
            &self.mode,
            Mode::Edit { original_auth, .. } if (*original_auth == AuthKind::PrivateKey) != current_is_key
        );

        let password = if !current_is_key {
            if switched_auth || self.password_touched || !preserve_password {
                Some(self.password.clone())
            } else if let Mode::Edit {
                existing_password: Some(p),
                ..
            } = &self.mode
            {
                Some(p.clone())
            } else {
                Some(self.password.clone())
            }
        } else {
            None
        };

        let private_key = if current_is_key {
            if switched_auth || self.private_key_touched || !preserve_key {
                Some(self.private_key.clone())
            } else if let Mode::Edit {
                existing_private_key: Some(b),
                ..
            } = &self.mode
            {
                Some(b.clone())
            } else {
                Some(self.private_key.clone())
            }
        } else {
            None
        };

        let passphrase = if current_is_key {
            if switched_auth {
                // Fresh auth — passphrase comes from the new input only.
                if self.passphrase.is_empty() {
                    None
                } else {
                    Some(self.passphrase.clone())
                }
            } else if self.passphrase_touched {
                if self.passphrase.is_empty() {
                    None
                } else {
                    Some(self.passphrase.clone())
                }
            } else if preserve_pass {
                if let Mode::Edit {
                    existing_passphrase: Some(p),
                    ..
                } = &self.mode
                {
                    Some(p.clone())
                } else {
                    None
                }
            } else if self.passphrase.is_empty() {
                None
            } else {
                Some(self.passphrase.clone())
            }
        } else {
            None
        };

        (password, private_key, passphrase)
    }
}

// ─── tiny utilities ────────────────────────────────────────────────

/// RFC 4122 v4 UUID using `getrandom` (already a transitive dep via
/// serde_json / others on iOS; if it isn't, fall back to a `SystemTime`-
/// seeded pseudo-random). We keep our own to avoid a new crate dep.
fn uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes);
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

fn fill_random(buf: &mut [u8]) {
    // Hash-based PRNG seeded from a counter + current nanoseconds.
    // Good enough for UUIDs (the Keychain uses them as opaque keys);
    // for cryptographic randomness we'd reach for `SecRandomCopyBytes`.
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xDEAD_BEEF);
    let bump = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut state = nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(bump);
    for b in buf.iter_mut() {
        state = state.wrapping_mul(0x5851_F42D_4C95_7F2D).wrapping_add(1);
        *b = ((state >> 33) & 0xFF) as u8;
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n =
            (u32::from(bytes[i]) << 16) | (u32::from(bytes[i + 1]) << 8) | u32::from(bytes[i + 2]);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = u32::from(bytes[i]) << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = (u32::from(bytes[i]) << 16) | (u32::from(bytes[i + 1]) << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

/// Process-wide singleton. Main-thread-only in practice (`@MainActor`
/// Swift callers); the `Mutex` is uncontended.
pub static VM: Mutex<ConnectFormVM> = Mutex::new(ConnectFormVM::new());

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> ConnectFormVM {
        let mut vm = ConnectFormVM::new();
        vm.reset_to_add();
        vm
    }

    #[test]
    fn add_mode_default_port() {
        let vm = fresh();
        assert_eq!(vm.port, "22");
        assert!(matches!(vm.mode, Mode::Add));
    }

    #[test]
    fn add_mode_validate_blank_rejects() {
        let vm = fresh();
        assert!(vm.validate().is_some());
        assert!(!vm.can_save());
    }

    #[test]
    fn add_mode_port_out_of_range_rejected() {
        let mut vm = fresh();
        vm.set_host("h".into());
        vm.set_username("u".into());
        vm.set_password("p".into());
        vm.set_port_text("70000".into());
        let msg = vm.validate().expect("should be invalid");
        assert!(msg.to_lowercase().contains("port"));
    }

    #[test]
    fn add_mode_zero_port_rejected() {
        let mut vm = fresh();
        vm.set_host("h".into());
        vm.set_username("u".into());
        vm.set_password("p".into());
        vm.set_port_text("0".into());
        assert!(vm.validate().is_some());
    }

    #[test]
    fn add_mode_valid_password_path() {
        let mut vm = fresh();
        vm.set_host("h".into());
        vm.set_username("u".into());
        vm.set_password("p".into());
        let outcome = vm.try_save().expect("valid form");
        assert_eq!(outcome.host, "h");
        assert_eq!(outcome.port, 22);
        assert_eq!(outcome.username, "u");
        assert!(!outcome.auth_is_key);
        assert_eq!(outcome.password.as_deref(), Some("p"));
        assert!(outcome.private_key_base64.is_none());
    }

    #[test]
    fn add_mode_key_requires_key_file() {
        let mut vm = fresh();
        vm.set_host("h".into());
        vm.set_username("u".into());
        vm.set_using_key(true);
        assert!(vm.try_save().is_err());
        vm.set_private_key_bytes(b"KEYBYTES".to_vec());
        let outcome = vm.try_save().expect("now valid");
        assert!(outcome.auth_is_key);
        assert_eq!(outcome.private_key_base64.as_deref(), Some("S0VZQllURVM="));
    }

    #[test]
    fn host_paste_splits_port() {
        let mut vm = fresh();
        vm.set_host("bastion.example.com:2222".into());
        vm.set_username("ops".into());
        vm.set_password("p".into());
        let outcome = vm.try_save().expect("valid");
        assert_eq!(outcome.host, "bastion.example.com");
        assert_eq!(outcome.port, 2222);
    }

    #[test]
    fn ipv6_bracketed_splits() {
        let mut vm = fresh();
        vm.set_host("[::1]:2222".into());
        vm.set_username("ops".into());
        vm.set_password("p".into());
        let outcome = vm.try_save().expect("valid");
        assert_eq!(outcome.host, "::1");
        assert_eq!(outcome.port, 2222);
    }

    #[test]
    fn host_username_trim() {
        let mut vm = fresh();
        vm.set_host("  host.example.com  ".into());
        vm.set_username("  deploy  ".into());
        vm.set_password("p".into());
        let outcome = vm.try_save().expect("valid");
        assert_eq!(outcome.host, "host.example.com");
        assert_eq!(outcome.username, "deploy");
    }

    #[test]
    fn edit_mode_preserves_password_when_untouched() {
        let mut vm = fresh();
        vm.prefill_edit(
            "the-id".into(),
            "prod".into(),
            "h".into(),
            22,
            "u".into(),
            false,
            Some("kept".into()),
            None,
            None,
        );
        vm.set_label("prod-renamed".into());
        let outcome = vm.try_save().expect("valid");
        assert_eq!(outcome.label, "prod-renamed");
        assert_eq!(outcome.id, "the-id");
        assert_eq!(outcome.password.as_deref(), Some("kept"));
    }

    #[test]
    fn edit_mode_overwrites_password_when_touched() {
        let mut vm = fresh();
        vm.prefill_edit(
            "the-id".into(),
            "prod".into(),
            "h".into(),
            22,
            "u".into(),
            false,
            Some("old".into()),
            None,
            None,
        );
        vm.set_password("new".into());
        let outcome = vm.try_save().expect("valid");
        assert_eq!(outcome.password.as_deref(), Some("new"));
    }

    #[test]
    fn edit_mode_auth_switch_invalidates_secret() {
        let mut vm = fresh();
        vm.prefill_edit(
            "the-id".into(),
            "prod".into(),
            "h".into(),
            22,
            "u".into(),
            false,
            Some("kept".into()),
            None,
            None,
        );
        vm.set_using_key(true);
        // No key picked yet → must error.
        assert!(vm.try_save().is_err());
        vm.set_private_key_bytes(b"K".to_vec());
        let outcome = vm.try_save().expect("now valid");
        assert!(outcome.auth_is_key);
        assert_eq!(outcome.private_key_base64.as_deref(), Some("Sw=="));
        assert!(outcome.password.is_none());
    }

    #[test]
    fn edit_mode_preserves_key_and_passphrase_when_untouched() {
        let mut vm = fresh();
        vm.prefill_edit(
            "the-id".into(),
            "prod".into(),
            "h".into(),
            22,
            "u".into(),
            true,
            None,
            Some(b"KEY".to_vec()),
            Some("pp".into()),
        );
        let outcome = vm.try_save().expect("valid");
        assert!(outcome.auth_is_key);
        assert_eq!(outcome.private_key_base64.as_deref(), Some("S0VZ"));
        assert_eq!(outcome.passphrase.as_deref(), Some("pp"));
    }

    #[test]
    fn edit_mode_replaces_key_when_touched() {
        let mut vm = fresh();
        vm.prefill_edit(
            "the-id".into(),
            "prod".into(),
            "h".into(),
            22,
            "u".into(),
            true,
            None,
            Some(b"OLD".to_vec()),
            None,
        );
        vm.set_private_key_bytes(b"NEW".to_vec());
        let outcome = vm.try_save().expect("valid");
        assert_eq!(outcome.private_key_base64.as_deref(), Some("TkVX"));
    }

    #[test]
    fn validate_localizes_via_l10n() {
        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };
        let vm = fresh();
        let msg = vm.validate().expect("blank");
        assert!(!msg.is_empty());
    }

    #[test]
    fn has_password_reflects_existing_secret() {
        let mut vm = fresh();
        vm.prefill_edit(
            "id".into(),
            String::new(),
            "h".into(),
            22,
            "u".into(),
            false,
            Some("kept".into()),
            None,
            None,
        );
        assert!(vm.has_password());
        // Untouched after auth switch — cached secret is still observable.
        vm.set_using_key(true);
        assert!(vm.has_password());
    }

    #[test]
    fn save_outcome_round_trips_json() {
        let mut vm = fresh();
        vm.set_host("10.0.0.5".into());
        vm.set_username("yi".into());
        vm.set_password("hunter2".into());
        let outcome = vm.try_save().expect("valid");
        let json = serde_json::to_string(&outcome).expect("encode");
        let decoded: SaveOutcome = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.host, "10.0.0.5");
        assert!(!decoded.auth_is_key);
        assert_eq!(decoded.password.as_deref(), Some("hunter2"));
    }

    #[test]
    fn base64_encodes_padding_cases() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn uuid_v4_shape() {
        let s = uuid_v4();
        assert_eq!(s.len(), 36);
        assert_eq!(s.as_bytes()[14], b'4');
        let v = s.as_bytes()[19];
        assert!(matches!(v, b'8' | b'9' | b'A' | b'B'));
    }
}
