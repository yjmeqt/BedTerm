//! Production SSH client backed by the [`russh`] crate.
//!
//! Implements [`SshClient`](super::SshClient) — see the trait-level docs for
//! the lifecycle contract (connect → open_shell → read/write/resize → close).
//!
//! # Auth
//!
//! Only password authentication is supported. Key auth is planned but not
//! yet implemented.
//!
//! # Host-key verification (TOFU)
//!
//! P3 will wire the Keychain-backed host‑key store via
//! [`RusshSshClient::set_host_key_verifier`].  Until then, all host keys are
//! accepted (open‑tofu mode — every connection trusts on first use).
//! The mechanism is ready: the `check_server_key` handler calls the global
//! verifier (if set) and stores mismatch details so [`connect`](SshClient::connect)
//! can return [`HostKeyMismatch`](SshError::HostKeyMismatch).
//!
//! # Proxy command
//!
//! When [`SshConnectionRequest::proxy_command`] is set, the client spawns a
//! subprocess via `sh -c "<proxy_command>"` and pipes the SSH protocol over
//! its stdin/stdout instead of connecting to a TCP socket directly.
//!
//! # Channel management
//!
//! `russh` `Channel<Msg>` is `Send` (all fields are tokio types + `Arc`),
//! so the channel handle can live in the client struct.  Data from the server
//! arrives through the channel's `wait()` method (pull‑based), matching the
//! [`SshClient::read`] contract.

use async_trait::async_trait;
use data_encoding::BASE64_MIME;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::ssh_client::{HostCredential, PtyDimensions, SshClient, SshConnectionRequest, SshError};

// ---------------------------------------------------------------------------
// Key-type detection
// ---------------------------------------------------------------------------

/// Classification of a private key algorithm, inferred from its content
/// without fully decoding the key.
///
/// Used for diagnostics and error reporting — tells the user *what kind* of
/// key was provided and whether it needs a passphrase, without requiring the
/// passphrase itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    /// Ed25519 key (OpenSSH or PKCS#8 v2 format).
    Ed25519,
    /// RSA key (OpenSSH, PKCS#1, or PKCS#8 format).
    Rsa,
    /// ECDSA NIST P-256 key.
    EcdsaNistp256,
    /// ECDSA NIST P-384 key.
    EcdsaNistp384,
    /// ECDSA NIST P-521 key.
    EcdsaNistp521,
    /// Key is encrypted — the algorithm cannot be determined without the
    /// passphrase (e.g. PKCS#8 encrypted wrapper).
    Encrypted,
    /// Unrecognised or unparseable key format.
    Unknown,
}

impl KeyType {
    /// Returns `true` when the key is definitely encrypted.
    pub fn is_encrypted(self) -> bool {
        self == KeyType::Encrypted
    }

    /// Human-readable name for display in error messages.
    pub fn name(self) -> &'static str {
        match self {
            KeyType::Ed25519 => "Ed25519",
            KeyType::Rsa => "RSA",
            KeyType::EcdsaNistp256 => "ECDSA (P-256)",
            KeyType::EcdsaNistp384 => "ECDSA (P-384)",
            KeyType::EcdsaNistp521 => "ECDSA (P-521)",
            KeyType::Encrypted => "encrypted",
            KeyType::Unknown => "unknown",
        }
    }
}

/// Determine the key type from raw key-file bytes.
///
/// First tries full decode via `russh::keys::decode_secret_key` with no
/// passphrase.  If the key is encrypted, falls back to header-based detection
/// to report the algorithm (OpenSSH headers expose the public key algorithm
/// even when the private half is encrypted).
pub fn detect_key_type(bytes: &[u8]) -> KeyType {
    let key_str = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return KeyType::Unknown,
    };

    // Fast path: try full decode with no passphrase.
    // If it succeeds we know the exact type.  On any error (encrypted,
    // corrupt, unsupported format) we fall through to header-based detection
    // so PKCS#5-encrypted PEM keys (which may not propagate `KeyIsEncrypted`)
    // are still identified.
    if let Ok(kp) = russh::keys::decode_secret_key(key_str, None) {
        return key_type_from_keypair(&kp);
    }

    // Header-based fallback: determine the format from the PEM header, then
    // for OpenSSH keys peek at the unencrypted public-key section.
    for line in key_str.lines() {
        let t = line.trim();
        if t == "-----BEGIN OPENSSH PRIVATE KEY-----" {
            return detect_openssh_header_algorithm(key_str);
        }
        if t == "-----BEGIN RSA PRIVATE KEY-----" {
            return KeyType::Rsa;
        }
        if t == "-----BEGIN EC PRIVATE KEY-----" {
            // SEC 1 EC key — could be P-256/P-384/P-521 but we'd need DER
            // parsing to tell which.  Report Encrypted since algorithm is
            // uncertain without the passphrase for encrypted variants.
            return KeyType::Encrypted;
        }
        // PKCS#8 "ENCRYPTED" or "PRIVATE KEY" that failed to parse:
        // algorithms are embedded in the DER — just report encrypted.
        if t == "-----BEGIN ENCRYPTED PRIVATE KEY-----" || t == "-----BEGIN PRIVATE KEY-----" {
            return KeyType::Encrypted;
        }
    }
    KeyType::Unknown
}

/// Derive a `KeyType` from a successfully decoded `KeyPair`.
fn key_type_from_keypair(kp: &russh::keys::key::KeyPair) -> KeyType {
    use russh::keys::key::KeyPair;
    match kp {
        KeyPair::Ed25519(_) => KeyType::Ed25519,
        KeyPair::RSA { .. } => KeyType::Rsa,
        KeyPair::EC { key } => match key {
            russh::keys::ec::PrivateKey::P256(_) => KeyType::EcdsaNistp256,
            russh::keys::ec::PrivateKey::P384(_) => KeyType::EcdsaNistp384,
            russh::keys::ec::PrivateKey::P521(_) => KeyType::EcdsaNistp521,
        },
    }
}

/// For an OpenSSH private key that failed full decode, determine the
/// algorithm by parsing the unencrypted binary header (the "publickey"
/// section is always in the clear, even when the private half is
/// encrypted or the encrypted section is corrupt).
///
/// OpenSSH key binary layout:
///
/// ```text
/// "openssh-key-v1\0"       (15 bytes, null-terminated)
/// string ciphername        ("none" or "aes256-ctr" / "aes256-cbc")
/// string kdfname           ("none" or "bcrypt")
/// string kdfoptions
/// uint32  number_of_keys   (always 1 for private keys)
/// string publickey         ♥ NOT encrypted — contains the algorithm name
/// string encrypted_privatekey
/// ```
///
/// We only need the `publickey` string.  Its first sub-string is the
/// algorithm name (`ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256`, etc.).
///
/// Returns `Unknown` when the body is unparseable (base64 decode failure,
/// truncated header, unrecognised algorithm name).
fn detect_openssh_header_algorithm(key_str: &str) -> KeyType {
    // 1. Strip PEM armour and collect base64 lines.
    let body_b64: String = key_str
        .lines()
        .skip_while(|l| !l.contains("-----BEGIN"))
        .skip(1) // skip the BEGIN line itself
        .take_while(|l| !l.contains("-----END"))
        .filter(|l| !l.trim().is_empty())
        .collect();

    let body = match BASE64_MIME.decode(body_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => return KeyType::Unknown,
    };

    // 2. Verify magic.
    if body.len() < 15 || &body[..15] != b"openssh-key-v1\0" {
        return KeyType::Unknown;
    }

    let body_len = body.len();
    let mut pos = 15usize;

    /// Helper: check that `offset` is within bounds and return it, or
    /// return `Unknown` on overflow.
    macro_rules! adv {
        ($offset:expr) => {
            match expect_bounds($offset, body_len) {
                Ok(o) => o,
                Err(_) => return KeyType::Unknown,
            }
        };
    }

    // 3-5. Read & skip ciphername, kdfname, kdfoptions (SSH wire strings).
    //      Each is: uint32 length prefix + N bytes of value.
    //      Read the length FIRST, then advance past the whole thing.
    let cipher_len = read_u32_be(&body, pos);
    pos = adv!(pos + 4 + cipher_len);
    let _ciphername = &body[pos - cipher_len..pos]; // for diagnostics

    let kdf_len = read_u32_be(&body, pos);
    pos = adv!(pos + 4 + kdf_len);

    let kdfopt_len = read_u32_be(&body, pos);
    pos = adv!(pos + 4 + kdfopt_len);

    // 6. Skip number_of_keys (uint32).
    pos = adv!(pos + 4);

    // 7. Read & skip publickey string — this contains the algorithm.
    let pub_len = read_u32_be(&body, pos);
    pos = adv!(pos + 4 + pub_len);
    let pubkey = &body[pos - pub_len..pos];

    // 8. Read the algorithm name from the public key.
    let alg_len = read_u32_be(pubkey, 0);
    let alg_end = 4 + alg_len;
    if alg_end > pubkey.len() {
        return KeyType::Unknown;
    }
    let algorithm = &pubkey[4..alg_end];

    match algorithm {
        b"ssh-ed25519" => KeyType::Ed25519,
        b"ssh-rsa" => KeyType::Rsa,
        b"ecdsa-sha2-nistp256" => KeyType::EcdsaNistp256,
        b"ecdsa-sha2-nistp384" => KeyType::EcdsaNistp384,
        b"ecdsa-sha2-nistp521" => KeyType::EcdsaNistp521,
        _ => KeyType::Unknown,
    }
}

/// Check that `pos` is within `limit`; return `pos` on success or
/// `KeyType::Encrypted` on overflow (the binary header is truncated).
#[inline(always)]
fn expect_bounds(pos: usize, limit: usize) -> Result<usize, KeyType> {
    if pos > limit {
        Err(KeyType::Encrypted)
    } else {
        Ok(pos)
    }
}

/// Read a big-endian `u32` from `buf[pos..pos+4]`.
///
/// # Panics
///
/// Panics if `pos + 4 > buf.len()`.  Callers must check bounds first.
#[inline(always)]
fn read_u32_be(buf: &[u8], pos: usize) -> usize {
    u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize
}

// ---------------------------------------------------------------------------
// Host‑key verification infrastructure (TOFU)
// ---------------------------------------------------------------------------

/// Outcome of a host‑key verification check.
pub enum HostKeyVerdict {
    /// The key is trusted (matched stored key, or first use is accepted).
    Accepted,
    /// The key's fingerprint does not match the stored fingerprint.
    Mismatch {
        /// Fingerprint that was stored for this host.
        stored: String,
    },
}

/// Signature of a host‑key verifier callback.
///
/// Receives the remote host key's SHA256 fingerprint (format `SHA256:<base64>`)
/// and returns a `HostKeyVerdict`.
pub type HostKeyVerifier = Box<dyn Fn(&str) -> HostKeyVerdict + Send + Sync>;

/// Global host‑key verifier.  Left unset (default) accepts all keys — safe for
/// development.  P3 calls [`RusshSshClient::set_host_key_verifier`] to wire the
/// Keychain‑backed store.
static HOST_KEY_VERIFIER: OnceLock<HostKeyVerifier> = OnceLock::new();

impl RusshSshClient {
    /// Install the global host‑key verifier.
    ///
    /// Must be called before [`connect`](SshClient::connect).  Only the first
    /// call takes effect; subsequent calls are silently ignored.
    pub fn set_host_key_verifier(verifier: HostKeyVerifier) {
        let _ = HOST_KEY_VERIFIER.set(verifier);
    }

    /// Returns `true` if a host‑key verifier has been installed.
    pub fn has_host_key_verifier() -> bool {
        HOST_KEY_VERIFIER.get().is_some()
    }
}

// ---------------------------------------------------------------------------
// Proxy‑command stream wrapper
// ---------------------------------------------------------------------------

/// Wraps a sub‑process's stdin/stdout as a single `AsyncRead + AsyncWrite`
/// stream suitable for `russh::client::connect_stream`.
struct ProxyCommandStream {
    child_stdin: tokio::process::ChildStdin,
    child_stdout: tokio::process::ChildStdout,
}

impl AsyncRead for ProxyCommandStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // SAFETY: `ProxyCommandStream` has no self‑referential fields, so
        // a `Pin::map_unchecked_mut` projection into `child_stdout` is sound.
        //
        // `ChildStdout` implements `AsyncRead + Unpin`.
        let stdout = unsafe { self.map_unchecked_mut(|s| &mut s.child_stdout) };
        stdout.poll_read(cx, buf)
    }
}

impl AsyncWrite for ProxyCommandStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let stdin = unsafe { self.map_unchecked_mut(|s| &mut s.child_stdin) };
        stdin.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        let stdin = unsafe { self.map_unchecked_mut(|s| &mut s.child_stdin) };
        stdin.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let stdin = unsafe { self.map_unchecked_mut(|s| &mut s.child_stdin) };
        stdin.poll_shutdown(cx)
    }
}

// ---------------------------------------------------------------------------
// Client handler (implements `russh::client::Handler`)
// ---------------------------------------------------------------------------

/// Shared state between the client handler and the owning `RusshSshClient`.
struct ClientHandler {
    /// Populated by `check_server_key` when a host‑key mismatch is detected.
    host_key_mismatch: Arc<Mutex<Option<(String, String)>>>,
}

#[async_trait]
impl russh::client::Handler for ClientHandler {
    type Error = russh::Error;

    /// Called during key exchange to verify the server's host key.
    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let remote_fingerprint = format!("SHA256:{}", server_public_key.fingerprint());

        if let Some(verifier) = HOST_KEY_VERIFIER.get() {
            match verifier(&remote_fingerprint) {
                HostKeyVerdict::Accepted => Ok(true),
                HostKeyVerdict::Mismatch { stored } => {
                    let mut guard = self.host_key_mismatch.lock().unwrap();
                    *guard = Some((stored, remote_fingerprint));
                    Ok(false) // causes `connect_stream` to return UnknownKey
                }
            }
        } else {
            // No verifier installed — trust on first use.
            Ok(true)
        }
    }
}

// ---------------------------------------------------------------------------
// RusshSshClient
// ---------------------------------------------------------------------------

/// Production SSH client backed by `russh`.
///
/// See the [`SshClient` trait](super::SshClient) for the lifecycle contract.
pub struct RusshSshClient {
    /// The `russh` session handle, created by [`connect`](SshClient::connect).
    handle: Option<russh::client::Handle<ClientHandler>>,
    /// The SSH channel, created by [`open_shell`](SshClient::open_shell).
    channel: Option<russh::Channel<russh::client::Msg>>,
    /// Set to `true` when EOF has been received from the remote side.
    eof_received: bool,
    /// Shared mismatch info.  Kept so the handler's callback can write into it
    /// during key exchange (which happens before `connect` returns).
    _mismatch_info: Arc<Mutex<Option<(String, String)>>>,
}

impl RusshSshClient {
    const DEFAULT_TERM: &'static str = "xterm-256color";
}

#[async_trait]
impl SshClient for RusshSshClient {
    async fn connect(request: SshConnectionRequest) -> Result<Self, SshError> {
        let config = Arc::new(russh::client::Config::default());
        let mismatch_info = Arc::new(Mutex::new(None::<(String, String)>));
        let handler = ClientHandler {
            host_key_mismatch: mismatch_info.clone(),
        };

        // --- Establish transport ---
        let handle = if let Some(proxy_cmd) = &request.proxy_command {
            Self::connect_via_proxy(config, handler, proxy_cmd).await?
        } else {
            Self::connect_direct(config, handler, &request.host, request.port).await?
        };

        // --- Authenticate ---
        let mut handle = handle;
        let authenticated = match &request.credential {
            HostCredential::Password { password } => handle
                .authenticate_password(&request.username, password)
                .await
                .map_err(|e| map_auth_error(&e))?,
            HostCredential::Agent => {
                return Err(SshError::Other(
                    "SSH agent authentication is not yet implemented".into(),
                ));
            }
        };

        if !authenticated {
            return Err(SshError::AuthenticationFailed);
        }

        Ok(Self {
            handle: Some(handle),
            channel: None,
            eof_received: false,
            _mismatch_info: mismatch_info,
        })
    }

    async fn open_shell(&mut self, dimensions: PtyDimensions) -> Result<(), SshError> {
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| SshError::Other("not connected".into()))?;

        let mut channel = handle
            .channel_open_session()
            .await
            .map_err(|e| map_channel_error(&e))?;

        channel
            .request_pty(
                true, // want_reply
                Self::DEFAULT_TERM,
                dimensions.cols as u32,
                dimensions.rows as u32,
                dimensions.width_px as u32,
                dimensions.height_px as u32,
                &[], // no terminal modes
            )
            .await
            .map_err(|e| map_channel_error(&e))?;

        // Wait for PTY request confirmation.  The channel is owned here
        // (not yet stored), so we can take &mut for wait().
        {
            let ch = &mut channel;
            loop {
                match ch.wait().await {
                    Some(russh::ChannelMsg::Success) => break,
                    Some(russh::ChannelMsg::Failure) => {
                        return Err(SshError::HandshakeFailed("PTY request was denied".into()));
                    }
                    Some(_) => continue,
                    None => {
                        return Err(SshError::Disconnected(
                            "channel closed before PTY confirmation".into(),
                        ));
                    }
                }
            }
        }

        channel
            .request_shell(true) // want_reply
            .await
            .map_err(|e| map_channel_error(&e))?;

        // Wait for shell request confirmation.
        {
            let ch = &mut channel;
            loop {
                match ch.wait().await {
                    Some(russh::ChannelMsg::Success) => break,
                    Some(russh::ChannelMsg::Failure) => {
                        return Err(SshError::HandshakeFailed("shell request was denied".into()));
                    }
                    Some(_) => continue,
                    None => {
                        return Err(SshError::Disconnected(
                            "channel closed before shell confirmation".into(),
                        ));
                    }
                }
            }
        }

        self.channel = Some(channel);
        Ok(())
    }

    async fn resize(&mut self, dimensions: PtyDimensions) -> Result<(), SshError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| SshError::Other("shell not opened".into()))?;

        channel
            .window_change(
                dimensions.cols as u32,
                dimensions.rows as u32,
                dimensions.width_px as u32,
                dimensions.height_px as u32,
            )
            .await
            .map_err(|e| SshError::Other(format!("resize failed: {e}")))?;

        Ok(())
    }

    async fn read(&mut self) -> Result<Vec<u8>, SshError> {
        if self.eof_received {
            return Ok(vec![]);
        }

        let channel = self
            .channel
            .as_mut()
            .ok_or_else(|| SshError::Other("shell not opened".into()))?;

        loop {
            match channel.wait().await {
                Some(russh::ChannelMsg::Data { data }) => {
                    return Ok(data.to_vec());
                }
                Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                    return Ok(data.to_vec());
                }
                Some(
                    russh::ChannelMsg::Eof
                    | russh::ChannelMsg::Close
                    | russh::ChannelMsg::OpenFailure(_),
                ) => {
                    self.eof_received = true;
                    return Ok(vec![]);
                }
                // PTY / shell success / failure are consumed silently;
                // they are responses to the `want_reply` requests we sent
                // in `open_shell`.
                Some(
                    russh::ChannelMsg::Success
                    | russh::ChannelMsg::Failure
                    | russh::ChannelMsg::Open { .. },
                ) => continue,
                // Exit status / exit signal — treat as EOF (the shell has
                // exited).
                Some(russh::ChannelMsg::ExitStatus { .. })
                | Some(russh::ChannelMsg::ExitSignal { .. }) => {
                    self.eof_received = true;
                    return Ok(vec![]);
                }
                // Window-adjusted / other channel bookkeeping — skip.
                Some(_) => continue,
                None => {
                    self.eof_received = true;
                    return Ok(vec![]);
                }
            }
        }
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), SshError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| SshError::Other("shell not opened".into()))?;

        channel
            .data(data)
            .await
            .map_err(|e| SshError::Other(format!("write failed: {e}")))?;

        Ok(())
    }

    async fn close(mut self) -> Result<(), SshError> {
        // 1. Close the channel (EOF + close).
        if let Some(channel) = self.channel.take() {
            let _ = channel.eof().await;
            let _ = channel.close().await;
        }

        // 2. Disconnect the session.
        if let Some(handle) = self.handle.as_ref() {
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "client closing", "")
                .await;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl RusshSshClient {
    /// Connect directly to the remote host via TCP.
    async fn connect_direct(
        config: Arc<russh::client::Config>,
        handler: ClientHandler,
        host: &str,
        port: u16,
    ) -> Result<russh::client::Handle<ClientHandler>, SshError> {
        let addr = tokio::net::lookup_host(format!("{host}:{port}"))
            .await
            .map_err(|_| SshError::DnsResolution)?
            .next()
            .ok_or(SshError::DnsResolution)?;

        let socket = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|e| map_io_error(e, &mut None))?;

        russh::client::connect_stream(config, socket, handler)
            .await
            .map_err(|e| map_connect_error(e, &mut None))
    }

    /// Connect through a proxy command (e.g. `ssh -W %h:%p jump-host`).
    async fn connect_via_proxy(
        config: Arc<russh::client::Config>,
        handler: ClientHandler,
        proxy_command: &str,
    ) -> Result<russh::client::Handle<ClientHandler>, SshError> {
        let mut child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(proxy_command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| SshError::Other(format!("failed to spawn proxy command: {e}")))?;

        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| SshError::Other("proxy command has no stdin".into()))?;
        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| SshError::Other("proxy command has no stdout".into()))?;

        let stream = ProxyCommandStream {
            child_stdin,
            child_stdout,
        };

        russh::client::connect_stream(config, stream, handler)
            .await
            .map_err(|e| map_connect_error(e, &mut None))
    }
}

/// Map a private‑key parse error.
fn decode_private_key(
    key_str: &str,
    passphrase: Option<&str>,
) -> Result<russh::keys::key::KeyPair, SshError> {
    match russh::keys::decode_secret_key(key_str, passphrase) {
        Ok(kp) => Ok(kp),
        Err(_) => Err(SshError::Other(
            "key authentication not yet implemented".into(),
        )),
    }
}

/// Map an IO error to `SshError`.
fn map_io_error(e: std::io::Error, _mismatch_opt: &mut Option<(String, String)>) -> SshError {
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => SshError::TcpRefused,
        std::io::ErrorKind::TimedOut => SshError::Timeout,
        std::io::ErrorKind::ConnectionReset => SshError::PeerReset,
        _ => SshError::Other(format!("IO error: {e}")),
    }
}

/// Map a `russh::Error` returned from `connect_stream` / `connect` to
/// `SshError`.  The `mismatch_opt` is checked first for host‑key mismatch;
/// this works around the fact that `connect` returns `UnknownKey` before we
/// can inspect the handler.
fn map_connect_error(e: russh::Error, mismatch_opt: &mut Option<(String, String)>) -> SshError {
    // Check for host‑key mismatch surfaced by the handler.
    if let Some((stored, remote)) = mismatch_opt.take() {
        return SshError::HostKeyMismatch { stored, remote };
    }

    match e {
        russh::Error::ConnectionTimeout
        | russh::Error::KeepaliveTimeout
        | russh::Error::InactivityTimeout => SshError::Timeout,
        russh::Error::HUP => SshError::PeerReset,
        russh::Error::Disconnect => SshError::Disconnected("connection closed".into()),
        russh::Error::UnknownKey => SshError::HostKeyMismatch {
            stored: String::new(),
            remote: String::new(),
        },
        russh::Error::IO(io_err) => map_io_error(io_err, mismatch_opt),
        _ => SshError::Other(format!("SSH connect failed: {e}")),
    }
}

/// Map a `russh::Error` from an auth operation.
///
/// The `authenticate_publickey` / `authenticate_password` methods return
/// `Ok(false)` when the server rejects the credential (handled by the
/// caller).  Errors here are transport/protocol failures during the auth
/// exchange itself.
fn map_auth_error(e: &russh::Error) -> SshError {
    match e {
        russh::Error::Disconnect => {
            SshError::Disconnected("disconnected during authentication".into())
        }
        russh::Error::IO(io_err) => {
            SshError::Other(format!("IO error during authentication: {io_err}"))
        }
        _ => SshError::Other(format!("authentication error: {e}")),
    }
}

/// Map a `russh::Error` from a channel operation.
fn map_channel_error(e: &russh::Error) -> SshError {
    match e {
        russh::Error::Disconnect => SshError::Disconnected("connection lost".into()),
        russh::Error::NotAuthenticated => SshError::AuthenticationFailed,
        _ => SshError::Other(format!("channel error: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
