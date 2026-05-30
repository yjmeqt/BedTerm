//! Pure-Rust SSH client trait and supporting types.
//!
//! Defines [`SshClient`], an async trait that mirrors the Swift `SSHClient`
//! protocol (`BedTermKit/Sources/BedTermKit/Core/SSH/SSHClient.swift`) but
//! follows Rust async conventions.
//!
//! # Host-testability
//!
//! Pure types only — no iOS dependency, no `cfg(target_os = "ios")` gate.
//! Compiles on macOS host for unit tests.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

pub use crate::pty::PtyDimensions;

/// Authentication credential for SSH connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HostCredential {
    /// Password-based authentication.
    #[serde(rename = "password")]
    Password { password: String },
    /// SSH agent forwarding (reserved for future use).
    #[serde(rename = "agent")]
    Agent,
}

/// Connection-request parameters for [`SshClient::connect`].
///
/// Transport-level parameters only. PTY dimensions are supplied later via
/// [`SshClient::open_shell`] so implementors can separate transport
/// establishment from channel opening.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConnectionRequest {
    /// Remote hostname or IP address.
    pub host: String,
    /// TCP port (default 22).
    pub port: u16,
    /// SSH username.
    pub username: String,
    /// Authentication credential.
    pub credential: HostCredential,
    /// Optional proxy/jump command, e.g. `ssh -W %h:%p jump-host`.
    /// When set, the client pipes the TCP stream through this command
    /// instead of connecting directly.
    pub proxy_command: Option<String>,
}

/// SSH error variants.
///
/// Mirrors the Swift `SSHError` enum plus a catch-all `Other` variant for
/// errors that do not fit the known categories (the C ABI's
/// `BtSSHResultOther`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshError {
    /// DNS resolution failed for the hostname.
    DnsResolution,
    /// TCP connection refused by the remote host.
    TcpRefused,
    /// Connection timed out.
    Timeout,
    /// SSH handshake (key exchange / protocol negotiation) failed.
    HandshakeFailed(String),
    /// Authentication rejected by the server.
    AuthenticationFailed,
    /// Host key fingerprint does not match the stored fingerprint.
    HostKeyMismatch {
        /// Fingerprint stored in the known-hosts database.
        stored: String,
        /// Fingerprint presented by the remote host.
        remote: String,
    },
    /// Connection closed unexpectedly by the remote side.
    Disconnected(String),
    /// TCP connection reset by peer.
    PeerReset,
    /// Remote shell process exited with a non-zero status.
    ShellExited(i32),
    /// Catch-all for any other error.
    Other(String),
}

impl SshError {
    /// Returns the corresponding [`BtSSHResultCode`] for this error variant.
    pub fn result_code(&self) -> crate::ssh_bridge::BtSSHResultCode {
        use crate::ssh_bridge::BtSSHResultCode;
        match self {
            Self::DnsResolution => BtSSHResultCode::BtSSHResultDnsResolution,
            Self::TcpRefused => BtSSHResultCode::BtSSHResultTcpRefused,
            Self::Timeout => BtSSHResultCode::BtSSHResultTimeout,
            Self::HandshakeFailed(_) => BtSSHResultCode::BtSSHResultHandshakeFailed,
            Self::AuthenticationFailed => BtSSHResultCode::BtSSHResultAuthenticationFailed,
            Self::HostKeyMismatch { .. } => BtSSHResultCode::BtSSHResultHostKeyMismatch,
            Self::Disconnected(_) => BtSSHResultCode::BtSSHResultDisconnected,
            Self::PeerReset => BtSSHResultCode::BtSSHResultPeerReset,
            Self::ShellExited(_) => BtSSHResultCode::BtSSHResultShellExited,
            Self::Other(_) => BtSSHResultCode::BtSSHResultOther,
        }
    }

    /// Returns a localization key for user-facing error messages.
    ///
    /// Swift looks up the key in its `Localizable.xcstrings` catalog
    /// via `String(localized:)`.
    pub fn localized_key(&self) -> &'static str {
        match self {
            Self::DnsResolution => "ssh.error.dns_resolution",
            Self::TcpRefused => "ssh.error.tcp_refused",
            Self::Timeout => "ssh.error.timeout",
            Self::HandshakeFailed(_) => "ssh.error.handshake_failed",
            Self::AuthenticationFailed => "ssh.error.authentication_failed",
            Self::HostKeyMismatch { .. } => "ssh.error.host_key_mismatch",
            Self::Disconnected(_) => "ssh.error.disconnected",
            Self::PeerReset => "ssh.error.peer_reset",
            Self::ShellExited(_) => "ssh.error.shell_exited",
            Self::Other(_) => "ssh.error.other",
        }
    }

    /// Constructs an `SshError` from a [`BtSSHResultCode`] and an optional
    /// detail string.
    ///
    /// For variants that carry structured data beyond a single string
    /// (e.g. [`HostKeyMismatch`](SshError::HostKeyMismatch) which needs
    /// both stored and remote fingerprints), the detail string is stored
    /// in the first field and the second is left empty. Callers who need
    /// the full two-field variant should construct it directly.
    pub fn from_result_code(
        code: crate::ssh_bridge::BtSSHResultCode,
        detail: Option<&str>,
    ) -> Self {
        use crate::ssh_bridge::BtSSHResultCode;
        match code {
            BtSSHResultCode::BtSSHResultOk => Self::Other("success".into()),
            BtSSHResultCode::BtSSHResultDnsResolution => Self::DnsResolution,
            BtSSHResultCode::BtSSHResultTcpRefused => Self::TcpRefused,
            BtSSHResultCode::BtSSHResultTimeout => Self::Timeout,
            BtSSHResultCode::BtSSHResultHandshakeFailed => {
                Self::HandshakeFailed(detail.unwrap_or("").to_string())
            }
            BtSSHResultCode::BtSSHResultAuthenticationFailed => Self::AuthenticationFailed,
            BtSSHResultCode::BtSSHResultHostKeyMismatch => Self::HostKeyMismatch {
                stored: detail.unwrap_or("").to_string(),
                remote: String::new(),
            },
            BtSSHResultCode::BtSSHResultDisconnected => {
                Self::Disconnected(detail.unwrap_or("").to_string())
            }
            BtSSHResultCode::BtSSHResultPeerReset => Self::PeerReset,
            BtSSHResultCode::BtSSHResultShellExited => {
                let exit_code = detail.and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1);
                Self::ShellExited(exit_code)
            }
            BtSSHResultCode::BtSSHResultOther => Self::Other(detail.unwrap_or("").to_string()),
        }
    }
}

impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DnsResolution => write!(f, "DNS resolution failed"),
            Self::TcpRefused => write!(f, "TCP connection refused"),
            Self::Timeout => write!(f, "connection timed out"),
            Self::HandshakeFailed(reason) => write!(f, "SSH handshake failed: {reason}"),
            Self::AuthenticationFailed => write!(f, "authentication failed"),
            Self::HostKeyMismatch { stored, remote } => {
                write!(f, "host key mismatch (stored: {stored}, remote: {remote})")
            }
            Self::Disconnected(reason) => write!(f, "disconnected: {reason}"),
            Self::PeerReset => write!(f, "connection reset by peer"),
            Self::ShellExited(code) => write!(f, "shell exited ({code})"),
            Self::Other(msg) => write!(f, "SSH error: {msg}"),
        }
    }
}

impl std::error::Error for SshError {}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstract SSH client connection.
///
/// Mirrors the Swift `SSHClient` protocol but exposes a Rust-native async
/// interface with separate transport-establishment and shell-opening steps.
///
/// # Lifecycle
///
/// 1. [`connect`](SshClient::connect) — resolve hostname, establish TCP,
///    perform SSH key exchange, and authenticate.
/// 2. [`open_shell`](SshClient::open_shell) — open an SSH channel with a
///    PTY and request a remote shell (or any single-channel program).
/// 3. [`write`](SshClient::write) / [`read`](SshClient::read) — bidirectional
///    data exchange against the remote shell.
/// 4. [`resize`](SshClient::resize) — notify the remote PTY of viewport
///    dimension changes.
/// 5. [`close`](SshClient::close) — close the channel and TCP connection
///    gracefully. Consumes `self`.
///
/// # Send + Sync
///
/// Every method takes `&mut self` (no overlapping operations). The trait
/// bounds `Send` so implementors can be moved across threads (e.g. to a
/// dedicated IO task).
///
#[async_trait]
pub trait SshClient: Send {
    /// Establish the SSH transport connection and authenticate.
    ///
    /// On success the caller must still call [`open_shell`](SshClient::open_shell)
    /// to open a PTY channel. Returns `HostKeyMismatch` when the remote
    /// fingerprint does not match the persisted known-host key; the caller
    /// should surface this to the user for confirmation.
    async fn connect(request: SshConnectionRequest) -> Result<Self, SshError>
    where
        Self: Sized;

    /// Open a PTY shell channel on the established connection.
    ///
    /// Sends an SSH `channel-open`, then `pty-req`, then `shell` request.
    /// After this succeeds, [`read`](SshClient::read) and
    /// [`write`](SshClient::write) are operational.
    async fn open_shell(&mut self, dimensions: PtyDimensions) -> Result<(), SshError>;

    /// Resize the remote PTY.
    async fn resize(&mut self, dimensions: PtyDimensions) -> Result<(), SshError>;

    /// Read the next chunk of data from the SSH channel.
    ///
    /// Returns an empty `Vec<u8>` when the channel has closed (end of data).
    async fn read(&mut self) -> Result<Vec<u8>, SshError>;

    /// Write data to the SSH channel (stdin of the remote shell).
    async fn write(&mut self, data: &[u8]) -> Result<(), SshError>;

    /// Close the SSH connection gracefully.
    ///
    /// Sends EOF and close on the channel, then disconnects the transport.
    /// Consumes `self` to prevent use-after-close.
    async fn close(self) -> Result<(), SshError>
    where
        Self: Sized;
}

// ---------------------------------------------------------------------------
// Sub-modules
// ---------------------------------------------------------------------------

/// Production SSH client backed by the `russh` crate.
pub(crate) mod russh_impl;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
