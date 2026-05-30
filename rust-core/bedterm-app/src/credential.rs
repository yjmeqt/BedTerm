//! Credential and auth-method types with JSON serialization support.
//!
//! Mirrors the Swift-side `HostCredential` / `AuthMethod` Codable types
//! that the connect form serializes into Keychain blobs.  Pure logic —
//! compiles and unit-tests on macOS host.
//!
//! Also defines the canonical [`SshConnectionRequest`] struct that matches
//! the Swift `SSHConnectionRequest` — the FFI connect path deserialises
//! from this JSON schema.
//!
//! # JSON schema
//!
//! `HostCredential` uses a serde internally-tagged enum so the JSON
//! can be round-tripped without a separate discriminator field:
//!
//! ```json
//! {"type": "password", "password": "s3cret"}
//! {"type": "private_key", "private_key": "LS0t…", "passphrase": null}
//! {"type": "agent"}
//! ```

use serde::{Deserialize, Serialize};

use crate::pty::PtyDimensions;

// ---------------------------------------------------------------------------
// AuthMethod
// ---------------------------------------------------------------------------

/// Authentication method type, without the credential payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Password,
    Agent,
}

// ---------------------------------------------------------------------------
// HostCredential (serde-enabled)
// ---------------------------------------------------------------------------

/// SSH authentication credential with JSON serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HostCredential {
    /// Password-based authentication.
    #[serde(rename = "password")]
    Password { password: String },
    /// SSH agent forwarding.
    #[serde(rename = "agent")]
    Agent,
}

impl HostCredential {
    pub fn auth_method(&self) -> AuthMethod {
        match self {
            Self::Password { .. } => AuthMethod::Password,
            Self::Agent => AuthMethod::Agent,
        }
    }

    pub const fn is_password(&self) -> bool {
        matches!(self, Self::Password { .. })
    }

    pub const fn is_agent(&self) -> bool {
        matches!(self, Self::Agent)
    }

    pub fn password(&self) -> Option<&str> {
        match self {
            Self::Password { password } => Some(password.as_str()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ConnectionFields — host/port/username + credential
// ---------------------------------------------------------------------------

/// Connection-target fields plus credential.
///
/// Mirrors the Swift `HostCredential` struct that flows through the Keychain
/// serialization path.  Pure serde container — no state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionFields {
    /// Remote hostname or IP address.
    pub host: String,
    /// TCP port (default 22).
    pub port: u16,
    /// SSH username.
    pub username: String,
    /// Authentication credential.
    pub credential: HostCredential,
}

// ---------------------------------------------------------------------------
// SshConnectionRequest — full canonical connection request
// ---------------------------------------------------------------------------

/// Full connection-request parameters for a terminal session.
///
/// Mirrors the Swift `SSHConnectionRequest` struct (`SSHClient.swift`).
/// This is the canonical JSON schema for the connect FFI boundary:
///
/// - `credential` bundles host/port/username + auth (matches Swift's
///   `HostCredential` struct).
/// - `initial_pty` defines the initial viewport dimensions.
/// - `bootstrap_payload` is written into the channel immediately after
///   opening (e.g. a shell-integration script).
///
/// All fields derive `Serialize` + `Deserialize` so a complete request can
/// be round-tripped through JSON for testing and debugging.
///
/// # Host-testability
///
/// Pure types — no iOS dependency, no `cfg(target_os = "ios")` gate.
/// Compiles on macOS host for unit tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConnectionRequest {
    /// Authentication credential (host, port, username, auth method).
    pub credential: ConnectionFields,
    /// Initial PTY dimensions in character cells.
    pub initial_pty: PtyDimensions,
    /// Optional bootstrap payload sent to the shell immediately after
    /// opening. `None` means no payload. Skipped in serialisation when
    /// absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_payload: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
