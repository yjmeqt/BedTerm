//! Pure-Rust SSH client trait and supporting types.
//!
//! Defines [`SshClient`], an async trait that mirrors the Swift `SSHClient`
//! protocol (`BedTermKit/Sources/BedTermKit/Core/SSH/SSHClient.swift`) but
//! follows Rust async conventions. The russh-based production implementation
//! lands in P2; a lightweight mock implements [`SshClient`] for host-side
//! unit tests without spinning up a real SSH server.
//!
//! # Relationship to `ssh_bridge`
//!
//! [`ssh_bridge`](super::ssh_bridge) is the *old* C-vtable FFI bridge that
//! lets Rust call back into Swift's Citadel-based `SSHClient`. It stays
//! alive until `TerminalSession` itself moves to Rust. Once that lands,
//! this module's [`SshClient`] trait drives both the real `russh` client
//! and any mock implementations — no FFI vtable needed.
//!
//! # FFI export (P3)
//!
//! The trait is designed for eventual FFI export. P3 will add
//! `#[derive(uniffi::Object)]` (or hand-rolled C conversion functions on
//! the concrete implementations) so Swift call sites can `await` through
//! the same async interface.
//!
//! # Host-testability
//!
//! Pure types only — no iOS dependency, no `cfg(target_os = "ios")` gate.
//! Compiles on macOS host for unit tests.

use async_trait::async_trait;

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// Terminal PTY dimensions in character cells, plus optional pixel hints.
///
/// Mirrors the Swift `PTYDimensions` struct but also carries pixel
/// dimensions, which the SSH `pty-req` and `window-change` channel requests
/// pass to the remote side for proper full-screen app layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtyDimensions {
    /// Number of character columns (width in cells).
    pub cols: u16,
    /// Number of character rows (height in cells).
    pub rows: u16,
    /// Pixel width of the terminal viewport (may be 0 if unknown).
    pub width_px: u16,
    /// Pixel height of the terminal viewport (may be 0 if unknown).
    pub height_px: u16,
}

impl PtyDimensions {
    /// Convenience constructor from cols/rows only (pixel hints default to 0).
    pub const fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            width_px: 0,
            height_px: 0,
        }
    }

    /// Full constructor including pixel dimensions.
    pub const fn with_pixels(cols: u16, rows: u16, width_px: u16, height_px: u16) -> Self {
        Self {
            cols,
            rows,
            width_px,
            height_px,
        }
    }
}

/// Authentication credential for SSH connections.
///
/// Mirrors the Swift `HostCredential.AuthMethod` enum. The host, port, and
/// username live in [`SshConnectionRequest`] — only the auth secret payload
/// is modelled here.
#[derive(Debug, Clone)]
pub enum HostCredential {
    /// Password-based authentication.
    Password {
        /// The password to send.
        password: String,
    },
    /// Public-key authentication (ed25519 or RSA).
    PrivateKey {
        /// Raw OpenSSH-format private key bytes (PEM or OpenSSH v1/v2).
        private_key: Vec<u8>,
        /// Optional passphrase. `None` means the key is unencrypted.
        passphrase: Option<String>,
    },
    /// SSH agent forwarding (not yet implemented in the Swift side;
    /// reserved for future use).
    Agent,
}

/// Connection-request parameters for [`SshClient::connect`].
///
/// Transport-level parameters only. PTY dimensions are supplied later via
/// [`SshClient::open_shell`] so implementors can separate transport
/// establishment from channel opening.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
    /// Private key data could not be parsed.
    PrivateKeyParse,
    /// Private key is encrypted and requires a passphrase.
    PrivateKeyPassphraseRequired,
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
            Self::PrivateKeyParse => BtSSHResultCode::BtSSHResultPrivateKeyParse,
            Self::PrivateKeyPassphraseRequired => {
                BtSSHResultCode::BtSSHResultPrivateKeyPassphraseRequired
            }
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
            Self::PrivateKeyParse => "ssh.error.private_key_parse",
            Self::PrivateKeyPassphraseRequired => "ssh.error.private_key_passphrase_required",
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
            BtSSHResultCode::BtSSHResultPrivateKeyParse => Self::PrivateKeyParse,
            BtSSHResultCode::BtSSHResultPrivateKeyPassphraseRequired => {
                Self::PrivateKeyPassphraseRequired
            }
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
            Self::PrivateKeyParse => write!(f, "cannot parse private key"),
            Self::PrivateKeyPassphraseRequired => write!(f, "private key requires a passphrase"),
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
/// # Mockability
///
/// The trait is designed for both real `russh`-backed implementations and
/// lightweight mock/simulated clients for unit tests — same approach as
/// the existing Swift `MockSSHClient`.
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

/// In-process mock SSH client for UI tests.
pub(crate) mod mock_impl;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that `PtyDimensions` derives Debug, Clone, Copy, PartialEq,
    /// Eq — needed by the trait methods and test assertions.
    #[test]
    fn pty_dimensions_derive() {
        let a = PtyDimensions::new(80, 24);
        let b = PtyDimensions::with_pixels(80, 24, 402, 874);
        let c = a;
        assert_eq!(a, c);
        assert_ne!(a, b);
        assert_eq!(b.cols, 80);
        assert_eq!(b.rows, 24);
        assert_eq!(b.width_px, 402);
        assert_eq!(b.height_px, 874);
    }

    /// Verifies that `SshError` Display and Error impls produce reasonable
    /// messages for each variant.
    #[test]
    fn ssh_error_display() {
        let cases: &[(SshError, &str)] = &[
            (SshError::DnsResolution, "DNS resolution failed"),
            (SshError::TcpRefused, "TCP connection refused"),
            (SshError::Timeout, "connection timed out"),
            (
                SshError::HandshakeFailed("bad version".into()),
                "SSH handshake failed: bad version",
            ),
            (SshError::AuthenticationFailed, "authentication failed"),
            (SshError::PrivateKeyParse, "cannot parse private key"),
            (
                SshError::PrivateKeyPassphraseRequired,
                "private key requires a passphrase",
            ),
            (
                SshError::HostKeyMismatch {
                    stored: "abc".into(),
                    remote: "def".into(),
                },
                "host key mismatch (stored: abc, remote: def)",
            ),
            (SshError::Disconnected("bye".into()), "disconnected: bye"),
            (SshError::PeerReset, "connection reset by peer"),
            (SshError::ShellExited(1), "shell exited (1)"),
            (
                SshError::Other("something broke".into()),
                "SSH error: something broke",
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(format!("{err}"), *expected, "Display mismatch for {err:?}");
            assert!(
                std::error::Error::source(err).is_none(),
                "SshError should not wrap another error"
            );
        }
    }

    /// Verifies that `SshConnectionRequest` and `HostCredential` derive
    /// Debug + Clone (required for ergonomic use).
    #[test]
    fn credential_debug_clone() {
        let cred = HostCredential::Password {
            password: "sekret".into(),
        };
        let _cloned = cred.clone();
        let _debug = format!("{cred:?}");

        let key_cred = HostCredential::PrivateKey {
            private_key: b"key data".to_vec(),
            passphrase: None,
        };
        let _cloned_key = key_cred.clone();

        let agent_cred = HostCredential::Agent;
        let _cloned_agent = agent_cred.clone();

        let req = SshConnectionRequest {
            host: "example.com".into(),
            port: 2222,
            username: "user".into(),
            credential: cred,
            proxy_command: None,
        };
        let _cloned_req = req.clone();
        let _debug_req = format!("{req:?}");
    }

    // ── SshError::result_code ────────────────────────────────────────

    /// Verifies each `SshError` variant maps to the correct
    /// `BtSSHResultCode`.
    #[test]
    fn ssh_error_result_code() {
        use crate::ssh_bridge::BtSSHResultCode;

        let cases: &[(SshError, BtSSHResultCode)] = &[
            (
                SshError::DnsResolution,
                BtSSHResultCode::BtSSHResultDnsResolution,
            ),
            (SshError::TcpRefused, BtSSHResultCode::BtSSHResultTcpRefused),
            (SshError::Timeout, BtSSHResultCode::BtSSHResultTimeout),
            (
                SshError::HandshakeFailed("bad version".into()),
                BtSSHResultCode::BtSSHResultHandshakeFailed,
            ),
            (
                SshError::AuthenticationFailed,
                BtSSHResultCode::BtSSHResultAuthenticationFailed,
            ),
            (
                SshError::PrivateKeyParse,
                BtSSHResultCode::BtSSHResultPrivateKeyParse,
            ),
            (
                SshError::PrivateKeyPassphraseRequired,
                BtSSHResultCode::BtSSHResultPrivateKeyPassphraseRequired,
            ),
            (
                SshError::HostKeyMismatch {
                    stored: "abc".into(),
                    remote: "def".into(),
                },
                BtSSHResultCode::BtSSHResultHostKeyMismatch,
            ),
            (
                SshError::Disconnected("bye".into()),
                BtSSHResultCode::BtSSHResultDisconnected,
            ),
            (SshError::PeerReset, BtSSHResultCode::BtSSHResultPeerReset),
            (
                SshError::ShellExited(1),
                BtSSHResultCode::BtSSHResultShellExited,
            ),
            (
                SshError::Other("oops".into()),
                BtSSHResultCode::BtSSHResultOther,
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(
                err.result_code(),
                *expected,
                "result_code mismatch for {err:?}"
            );
        }
    }

    // ── SshError::localized_key ──────────────────────────────────────

    /// Verifies each `SshError` variant returns a non-empty localized
    /// key string.
    #[test]
    fn ssh_error_localized_key() {
        let cases: &[SshError] = &[
            SshError::DnsResolution,
            SshError::TcpRefused,
            SshError::Timeout,
            SshError::HandshakeFailed("".into()),
            SshError::AuthenticationFailed,
            SshError::PrivateKeyParse,
            SshError::PrivateKeyPassphraseRequired,
            SshError::HostKeyMismatch {
                stored: "".into(),
                remote: "".into(),
            },
            SshError::Disconnected("".into()),
            SshError::PeerReset,
            SshError::ShellExited(0),
            SshError::Other("".into()),
        ];
        for err in cases {
            let key = err.localized_key();
            assert!(!key.is_empty(), "localized_key is empty for {err:?}");
            assert!(
                key.starts_with("ssh.error."),
                "localized_key does not start with \"ssh.error.\": {key}"
            );
        }
    }

    // ── SshError::from_result_code ───────────────────────────────────

    /// Verifies round-trip: for every `BtSSHResultCode` variant (except
    /// `Ok` which maps to `Other`), constructing via `from_result_code`
    /// and reading back via `result_code` gives the original code.
    #[test]
    fn ssh_error_from_result_code_roundtrip() {
        use crate::ssh_bridge::BtSSHResultCode;

        let codes = [
            BtSSHResultCode::BtSSHResultDnsResolution,
            BtSSHResultCode::BtSSHResultTcpRefused,
            BtSSHResultCode::BtSSHResultTimeout,
            BtSSHResultCode::BtSSHResultHandshakeFailed,
            BtSSHResultCode::BtSSHResultAuthenticationFailed,
            BtSSHResultCode::BtSSHResultPrivateKeyParse,
            BtSSHResultCode::BtSSHResultPrivateKeyPassphraseRequired,
            BtSSHResultCode::BtSSHResultHostKeyMismatch,
            BtSSHResultCode::BtSSHResultDisconnected,
            BtSSHResultCode::BtSSHResultPeerReset,
            BtSSHResultCode::BtSSHResultShellExited,
            BtSSHResultCode::BtSSHResultOther,
        ];
        for code in &codes {
            let err = SshError::from_result_code(*code, None);
            assert_eq!(
                err.result_code(),
                *code,
                "round-trip failed for {code:?} -> {err:?}"
            );
        }
    }

    /// Verifies that `from_result_code` preserves the detail string for
    /// variants that carry one.
    #[test]
    fn ssh_error_from_result_code_detail() {
        use crate::ssh_bridge::BtSSHResultCode;

        // HandshakeFailed with detail
        let err = SshError::from_result_code(
            BtSSHResultCode::BtSSHResultHandshakeFailed,
            Some("bad version"),
        );
        assert!(matches!(err, SshError::HandshakeFailed(ref r) if r == "bad version"));

        // Disconnected with detail
        let err = SshError::from_result_code(BtSSHResultCode::BtSSHResultDisconnected, Some("bye"));
        assert!(matches!(err, SshError::Disconnected(ref r) if r == "bye"));

        // ShellExited with detail (parsed as i32)
        let err = SshError::from_result_code(BtSSHResultCode::BtSSHResultShellExited, Some("42"));
        assert!(matches!(err, SshError::ShellExited(c) if c == 42));

        // ShellExited with unparseable detail defaults to -1
        let err = SshError::from_result_code(
            BtSSHResultCode::BtSSHResultShellExited,
            Some("not-a-number"),
        );
        assert!(matches!(err, SshError::ShellExited(c) if c == -1));
    }
}
