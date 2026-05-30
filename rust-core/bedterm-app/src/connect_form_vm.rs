#![allow(dead_code)]
//! Pure state machine for the connect-form (formerly Swift
//! `ConnectionFormViewModel`). Owns every form field, dirty tracking,
//! validation rules, and the secret-preservation logic for edit mode.
//!
//! No UIKit imports — this module is host-testable. The FFI singleton
//! lives in [`crate::ffi::connect_form_vm`] (iOS-gated). When building for
//! macOS the FFI consumer is not compiled, so the public API appears dead.
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
use std::ffi::{c_char, c_void};
use std::sync::Mutex;

/// Auth kind picked in the segmented control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthKind {
    Password,
    /// Key auth is not yet implemented — selecting this shows "Coming Soon".
    PrivateKey,
}

/// Mode the form was opened in.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Add,
    Edit {
        id: String,
        original_auth: AuthKind,
        existing_password: Option<String>,
    },
}

/// Resolved-fields output of a successful save attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveOutcome {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(rename = "authIsKey")]
    pub auth_is_key: bool,
    pub password: Option<String>,
}

pub type SaveResultFn = Option<unsafe extern "C" fn(*mut c_void, *const c_char)>;

pub struct ConnectFormVM {
    pub mode: Mode,
    pub label: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub is_using_key: bool,
    pub password: String,
    pub password_touched: bool,
    pub error_message: Option<String>,

    pub save_result_cb: SaveResultFn,
    pub save_result_ctx: usize,
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
            password_touched: false,
            error_message: None,
            save_result_cb: None,
            save_result_ctx: 0,
        }
    }

    pub fn set_callbacks(&mut self, save_result_cb: SaveResultFn, save_result_ctx: *mut c_void) {
        self.save_result_cb = save_result_cb;
        self.save_result_ctx = save_result_ctx as usize;
    }

    pub fn fire_save_result(&self, json: &str) {
        if let Some(cb) = self.save_result_cb {
            let c_json = std::ffi::CString::new(json).unwrap_or_default();
            unsafe { cb(self.save_result_ctx as *mut c_void, c_json.as_ptr()) };
        }
    }

    /// Reset to a blank Add-mode draft (default port 22).
    pub fn reset_to_add(&mut self) {
        *self = Self::new();
        self.port = String::from("22");
    }

    /// Prefill the form for an Edit-mode session.
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
        };
        self.label = label;
        self.host = host;
        self.port = port.to_string();
        self.username = username;
        self.is_using_key = auth_is_key;
        self.password.clear();
        self.password_touched = false;
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

    /// Key auth is not yet implemented; returns `false`.
    pub fn has_private_key(&self) -> bool {
        false
    }

    /// Key auth is not yet implemented; returns `false`.
    pub fn has_passphrase(&self) -> bool {
        false
    }

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
        self.password_touched
    }

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
            return Some(t("Key authentication coming soon."));
        }
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

        let password = self.resolve_password();

        Ok(SaveOutcome {
            id,
            label,
            host,
            port,
            username: user,
            auth_is_key: false,
            password,
        })
    }

    fn resolve_password(&self) -> Option<String> {
        let preserve_password = matches!(
            &self.mode,
            Mode::Edit {
                original_auth: AuthKind::Password,
                existing_password: Some(_),
                ..
            }
        );
        let switched_from_key = matches!(
            &self.mode,
            Mode::Edit {
                original_auth: AuthKind::PrivateKey,
                ..
            }
        );

        if switched_from_key || self.password_touched || !preserve_password {
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

/// Decode standard-base64 into raw bytes. Handles padding. Returns `None`
/// on invalid input (wrong length, non-base64 chars).
pub(crate) fn base64_decode(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() {
        return Some(Vec::new());
    }
    // Strip any whitespace (JSON may insert none, but be defensive).
    let clean: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if !clean.len().is_multiple_of(4) {
        return None;
    }
    let padding = clean.chars().rev().take_while(|&c| c == '=').count();
    let out_len = clean.len() / 4 * 3 - padding;
    let mut out = Vec::with_capacity(out_len);
    let table = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    for chunk in clean.as_bytes().chunks(4) {
        if chunk.len() != 4 {
            return None;
        }
        let a = table(chunk[0])?;
        let b = table(chunk[1])?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            table(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            table(chunk[3])?
        };
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            out.push(((b & 0x0F) << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            out.push(((c & 0x03) << 6) | d);
        }
    }
    Some(out)
}

impl From<SaveOutcome> for crate::hosts::model::SavedHost {
    fn from(outcome: SaveOutcome) -> Self {
        use crate::hosts::model::{AuthMethod, HostCredential, SavedHost};
        SavedHost {
            id: outcome.id,
            label: outcome.label,
            credential: HostCredential {
                host: outcome.host,
                port: outcome.port,
                username: outcome.username,
                auth: AuthMethod::Password(outcome.password.unwrap_or_default()),
            },
        }
    }
}

/// Process-wide singleton. Main-thread-only in practice (`@MainActor`
/// Swift callers); the `Mutex` is uncontended.
pub static VM: Mutex<ConnectFormVM> = Mutex::new(ConnectFormVM::new());
