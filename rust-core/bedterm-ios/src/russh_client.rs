//! Rust-owned production SSH client backed by `russh`.
//!
//! Swift owns the user-facing `SSHClient` protocol. This module owns the
//! transport and exposes a narrow blocking C ABI that Swift calls from
//! detached tasks so the main actor never waits on network I/O.

#![cfg(target_os = "ios")]

use std::ffi::{c_char, c_void, CStr, CString};
use std::io::ErrorKind;
use std::ptr;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use russh::client::{self, Msg};
use russh::{Channel, ChannelMsg, Disconnect};
use russh_keys::key::{KeyPair, PublicKey};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc;

use crate::host_key_store::{self, Verdict};
use crate::ssh_bridge::BtSSHResultCode;

pub type BtRusshOutputSink =
    unsafe extern "C" fn(ctx: *mut c_void, bytes: *const u8, len: usize);
pub type BtRusshCloseSink = unsafe extern "C" fn(ctx: *mut c_void);

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(dead_code, non_camel_case_types, clippy::enum_variant_names)]
pub enum BtRusshAuthKind {
    BtRusshAuthPassword = 0,
    BtRusshAuthPrivateKey = 1,
}

#[repr(C)]
pub struct BtRusshConnectRequest {
    pub host: *const c_char,
    pub port: u16,
    pub username: *const c_char,
    pub auth_kind: BtRusshAuthKind,
    pub password: *const c_char,
    pub private_key: *const u8,
    pub private_key_len: usize,
    pub passphrase: *const c_char,
    pub cols: u16,
    pub rows: u16,
    pub bootstrap_payload: *const c_char,
}

#[repr(C)]
pub struct BtRusshResult {
    pub code: BtSSHResultCode,
    /// NUL-terminated UTF-8 allocated by Rust. Free with
    /// `bt_russh_result_message_free`.
    pub message: *mut c_char,
    pub extra: i32,
}

pub struct BtRusshClient {
    runtime: Runtime,
    session: Mutex<Option<SessionState>>,
    output_sink: Option<BtRusshOutputSink>,
    close_sink: Option<BtRusshCloseSink>,
    callback_ctx: usize,
}

struct SessionState {
    tx: mpsc::UnboundedSender<SessionCommand>,
}

enum SessionCommand {
    Write(Vec<u8>, std_mpsc::Sender<ClientResult<()>>),
    Resize(u32, u32, std_mpsc::Sender<ClientResult<()>>),
    Disconnect(std_mpsc::Sender<ClientResult<()>>),
}

#[derive(Copy, Clone)]
struct Callbacks {
    output: Option<BtRusshOutputSink>,
    close: Option<BtRusshCloseSink>,
    ctx: usize,
}

#[derive(Debug)]
enum ClientError {
    MissingClient,
    InvalidRequest(&'static str),
    Timeout,
    AuthenticationFailed,
    PrivateKeyParse,
    PrivateKeyPassphraseRequired,
    HostKeyMismatch { stored: String, remote: String },
    Disconnected(String),
    ShellExited(i32),
    Russh(russh::Error),
    Io(std::io::Error),
}

type ClientResult<T> = Result<T, ClientError>;

impl From<russh::Error> for ClientError {
    fn from(value: russh::Error) -> Self {
        Self::Russh(value)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

struct ConnectRequest {
    host: String,
    port: u16,
    username: String,
    auth: AuthRequest,
    cols: u16,
    rows: u16,
    bootstrap_payload: Option<String>,
}

enum AuthRequest {
    Password(String),
    PrivateKey {
        pem: String,
        passphrase: Option<String>,
    },
}

struct HostKeyHandler {
    host: String,
    port: u16,
}

#[async_trait]
impl client::Handler for HostKeyHandler {
    type Error = ClientError;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> ClientResult<bool> {
        let remote = format!("SHA256:{}", server_public_key.fingerprint());
        match host_key_store::verify(&self.host, self.port, &remote) {
            Verdict::Match => Ok(true),
            Verdict::Unknown => {
                if host_key_store::save(&self.host, self.port, &remote) {
                    Ok(true)
                } else {
                    Err(ClientError::HostKeyMismatch {
                        stored: String::new(),
                        remote,
                    })
                }
            }
            Verdict::Mismatch { stored } => {
                Err(ClientError::HostKeyMismatch { stored, remote })
            }
        }
    }
}

impl BtRusshClient {
    fn new(
        output_sink: Option<BtRusshOutputSink>,
        close_sink: Option<BtRusshCloseSink>,
        callback_ctx: *mut c_void,
    ) -> ClientResult<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("bedterm-russh")
            .enable_all()
            .build()
            .map_err(ClientError::Io)?;
        Ok(Self {
            runtime,
            session: Mutex::new(None),
            output_sink,
            close_sink,
            callback_ctx: callback_ctx as usize,
        })
    }

    fn connect_blocking(&self, request: ConnectRequest) -> ClientResult<()> {
        self.disconnect_blocking().ok();
        let callbacks = Callbacks {
            output: self.output_sink,
            close: self.close_sink,
            ctx: self.callback_ctx,
        };
        let state = self.runtime.block_on(connect_inner(request, callbacks))?;
        let mut guard = self
            .session
            .lock()
            .map_err(|_| ClientError::Disconnected("session lock poisoned".into()))?;
        *guard = Some(state);
        Ok(())
    }

    fn with_session_command(
        &self,
        make: impl FnOnce(std_mpsc::Sender<ClientResult<()>>) -> SessionCommand,
    ) -> ClientResult<()> {
        let tx = {
            let guard = self
                .session
                .lock()
                .map_err(|_| ClientError::Disconnected("session lock poisoned".into()))?;
            guard.as_ref().ok_or(ClientError::MissingClient)?.tx.clone()
        };
        let (reply_tx, reply_rx) = std_mpsc::channel();
        tx.send(make(reply_tx))
            .map_err(|_| ClientError::Disconnected("session ended".into()))?;
        reply_rx
            .recv()
            .map_err(|_| ClientError::Disconnected("session ended".into()))?
    }

    fn write_blocking(&self, bytes: &[u8]) -> ClientResult<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.with_session_command(|reply| SessionCommand::Write(bytes.to_vec(), reply))
    }

    fn resize_blocking(&self, cols: u16, rows: u16) -> ClientResult<()> {
        self.with_session_command(|reply| {
            SessionCommand::Resize(u32::from(cols), u32::from(rows), reply)
        })
    }

    fn disconnect_blocking(&self) -> ClientResult<()> {
        let state = {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| ClientError::Disconnected("session lock poisoned".into()))?;
            guard.take()
        };
        let Some(state) = state else {
            return Ok(());
        };
        let (reply_tx, reply_rx) = std_mpsc::channel();
        state
            .tx
            .send(SessionCommand::Disconnect(reply_tx))
            .map_err(|_| ClientError::Disconnected("session ended".into()))?;
        reply_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| Ok(()))
    }
}

async fn connect_inner(request: ConnectRequest, callbacks: Callbacks) -> ClientResult<SessionState> {
    let config = Arc::new(client::Config::default());
    let handler = HostKeyHandler {
        host: request.host.clone(),
        port: request.port,
    };
    let mut handle = tokio::time::timeout(
        Duration::from_secs(5),
        client::connect(config, (request.host.as_str(), request.port), handler),
    )
    .await
    .map_err(|_| ClientError::Timeout)??;

    let authenticated = match request.auth {
        AuthRequest::Password(password) => {
            handle
                .authenticate_password(request.username.clone(), password)
                .await?
        }
        AuthRequest::PrivateKey { pem, passphrase } => {
            let key = parse_private_key(&pem, passphrase.as_deref())?;
            handle
                .authenticate_publickey(request.username.clone(), Arc::new(key))
                .await?
        }
    };
    if !authenticated {
        return Err(ClientError::AuthenticationFailed);
    }

    let channel = handle.channel_open_session().await?;
    channel
        .request_pty(
            false,
            "xterm-256color",
            u32::from(request.cols),
            u32::from(request.rows),
            0,
            0,
            &[],
        )
        .await?;
    channel.request_shell(false).await?;

    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(session_loop(
        handle,
        channel,
        rx,
        callbacks,
        request.bootstrap_payload,
    ));
    Ok(SessionState { tx })
}

fn parse_private_key(pem: &str, passphrase: Option<&str>) -> ClientResult<KeyPair> {
    match russh_keys::decode_secret_key(pem, passphrase) {
        Ok(key) => Ok(key),
        Err(err) => {
            if passphrase.is_none() && error_mentions_encryption(&err) {
                Err(ClientError::PrivateKeyPassphraseRequired)
            } else {
                Err(ClientError::PrivateKeyParse)
            }
        }
    }
}

fn error_mentions_encryption(err: &russh_keys::Error) -> bool {
    let text = err.to_string().to_lowercase();
    text.contains("bcrypt")
        || text.contains("cipher")
        || text.contains("decrypt")
        || text.contains("encrypted")
        || text.contains("kdf")
        || text.contains("passphrase")
        || text.contains("password")
}

async fn session_loop(
    handle: client::Handle<HostKeyHandler>,
    mut channel: Channel<Msg>,
    mut rx: mpsc::UnboundedReceiver<SessionCommand>,
    callbacks: Callbacks,
    mut bootstrap_payload: Option<String>,
) {
    loop {
        tokio::select! {
            Some(command) = rx.recv() => {
                let should_break = handle_command(&handle, &channel, command).await;
                if should_break {
                    break;
                }
            }
            msg = channel.wait() => {
                let Some(msg) = msg else { break };
                match msg {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                        if let Some(sink) = callbacks.output {
                            unsafe {
                                sink(callbacks.ctx as *mut c_void, data.as_ptr(), data.len());
                            }
                        }
                        if let Some(payload) = bootstrap_payload.take() {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            let _ = channel.data(payload.as_bytes()).await;
                        }
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        let _ = ClientError::ShellExited(exit_status as i32);
                        break;
                    }
                    ChannelMsg::Eof | ChannelMsg::Close => break,
                    _ => {}
                }
            }
            else => break,
        }
    }

    if let Some(close) = callbacks.close {
        unsafe {
            close(callbacks.ctx as *mut c_void);
        }
    }
}

async fn handle_command(
    handle: &client::Handle<HostKeyHandler>,
    channel: &Channel<Msg>,
    command: SessionCommand,
) -> bool {
    match command {
        SessionCommand::Write(bytes, reply) => {
            let result = channel.data(&bytes[..]).await.map_err(ClientError::Russh);
            let _ = reply.send(result);
            false
        }
        SessionCommand::Resize(cols, rows, reply) => {
            let result = channel
                .window_change(cols, rows, 0, 0)
                .await
                .map_err(ClientError::Russh);
            let _ = reply.send(result);
            false
        }
        SessionCommand::Disconnect(reply) => {
            let _ = channel.eof().await;
            let _ = channel.close().await;
            let result = handle
                .disconnect(Disconnect::ByApplication, "", "English")
                .await
                .map_err(ClientError::from);
            let _ = reply.send(result);
            true
        }
    }
}

fn result_from_error(error: ClientError) -> BtRusshResult {
    match error {
        ClientError::MissingClient => result(
            BtSSHResultCode::BtSSHResultDisconnected,
            Some("not connected".into()),
            0,
        ),
        ClientError::InvalidRequest(message) => {
            result(BtSSHResultCode::BtSSHResultOther, Some(message.into()), 0)
        }
        ClientError::Timeout => result(BtSSHResultCode::BtSSHResultTimeout, None, 0),
        ClientError::AuthenticationFailed => {
            result(BtSSHResultCode::BtSSHResultAuthenticationFailed, None, 0)
        }
        ClientError::PrivateKeyParse => {
            result(BtSSHResultCode::BtSSHResultPrivateKeyParse, None, 0)
        }
        ClientError::PrivateKeyPassphraseRequired => result(
            BtSSHResultCode::BtSSHResultPrivateKeyPassphraseRequired,
            None,
            0,
        ),
        ClientError::HostKeyMismatch { stored, remote } => result(
            BtSSHResultCode::BtSSHResultHostKeyMismatch,
            Some(format!("{stored}\n{remote}")),
            0,
        ),
        ClientError::Disconnected(reason) => {
            result(BtSSHResultCode::BtSSHResultDisconnected, Some(reason), 0)
        }
        ClientError::ShellExited(code) => {
            result(BtSSHResultCode::BtSSHResultShellExited, None, code)
        }
        ClientError::Russh(error) => result_from_russh(error),
        ClientError::Io(error) => result_from_io(error),
    }
}

fn result_from_russh(error: russh::Error) -> BtRusshResult {
    match error {
        russh::Error::ConnectionTimeout
        | russh::Error::KeepaliveTimeout
        | russh::Error::InactivityTimeout => result(BtSSHResultCode::BtSSHResultTimeout, None, 0),
        russh::Error::HUP | russh::Error::Disconnect => {
            result(BtSSHResultCode::BtSSHResultPeerReset, None, 0)
        }
        russh::Error::IO(io) => result_from_io(io),
        other => {
            let text = other.to_string();
            let lower = text.to_lowercase();
            if lower.contains("auth") {
                result(BtSSHResultCode::BtSSHResultAuthenticationFailed, None, 0)
            } else if lower.contains("resolve")
                || lower.contains("nodename")
                || lower.contains("domain name")
            {
                result(BtSSHResultCode::BtSSHResultDnsResolution, None, 0)
            } else if lower.contains("key exchange")
                || lower.contains("algorithm")
                || lower.contains("cipher")
                || lower.contains("mac")
            {
                result(BtSSHResultCode::BtSSHResultHandshakeFailed, Some(text), 0)
            } else {
                result(BtSSHResultCode::BtSSHResultDisconnected, Some(text), 0)
            }
        }
    }
}

fn result_from_io(error: std::io::Error) -> BtRusshResult {
    let kind = error.kind();
    match kind {
        ErrorKind::ConnectionRefused => result(BtSSHResultCode::BtSSHResultTcpRefused, None, 0),
        ErrorKind::TimedOut => result(BtSSHResultCode::BtSSHResultTimeout, None, 0),
        ErrorKind::ConnectionReset | ErrorKind::BrokenPipe => {
            result(BtSSHResultCode::BtSSHResultPeerReset, None, 0)
        }
        ErrorKind::NotFound | ErrorKind::AddrNotAvailable => {
            result(BtSSHResultCode::BtSSHResultDnsResolution, None, 0)
        }
        _ => {
            let text = error.to_string();
            let lower = text.to_lowercase();
            if lower.contains("nodename") || lower.contains("domain name") {
                result(BtSSHResultCode::BtSSHResultDnsResolution, None, 0)
            } else {
                result(BtSSHResultCode::BtSSHResultDisconnected, Some(text), 0)
            }
        }
    }
}

fn ok() -> BtRusshResult {
    result(BtSSHResultCode::BtSSHResultOk, None, 0)
}

fn result(code: BtSSHResultCode, message: Option<String>, extra: i32) -> BtRusshResult {
    BtRusshResult {
        code,
        message: message
            .and_then(|m| CString::new(m.replace('\0', " ")).ok())
            .map_or(ptr::null_mut(), CString::into_raw),
        extra,
    }
}

unsafe fn read_required_cstr(
    ptr: *const c_char,
    name: &'static str,
) -> ClientResult<String> {
    if ptr.is_null() {
        return Err(ClientError::InvalidRequest(name));
    }
    Ok(CStr::from_ptr(ptr).to_string_lossy().into_owned())
}

unsafe fn read_optional_cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

unsafe fn read_connect_request(req: *const BtRusshConnectRequest) -> ClientResult<ConnectRequest> {
    if req.is_null() {
        return Err(ClientError::InvalidRequest("missing connect request"));
    }
    let req = &*req;
    let host = read_required_cstr(req.host, "missing host")?;
    let username = read_required_cstr(req.username, "missing username")?;
    let auth = match req.auth_kind {
        BtRusshAuthKind::BtRusshAuthPassword => {
            AuthRequest::Password(read_required_cstr(req.password, "missing password")?)
        }
        BtRusshAuthKind::BtRusshAuthPrivateKey => {
            if req.private_key.is_null() || req.private_key_len == 0 {
                return Err(ClientError::PrivateKeyParse);
            }
            let bytes = std::slice::from_raw_parts(req.private_key, req.private_key_len);
            let pem = String::from_utf8(bytes.to_vec()).map_err(|_| ClientError::PrivateKeyParse)?;
            AuthRequest::PrivateKey {
                pem,
                passphrase: read_optional_cstr(req.passphrase),
            }
        }
    };
    Ok(ConnectRequest {
        host,
        port: req.port,
        username,
        auth,
        cols: req.cols.max(1),
        rows: req.rows.max(1),
        bootstrap_payload: read_optional_cstr(req.bootstrap_payload),
    })
}

#[no_mangle]
pub unsafe extern "C" fn bt_russh_client_create(
    output_sink: unsafe extern "C" fn(ctx: *mut c_void, bytes: *const u8, len: usize),
    close_sink: unsafe extern "C" fn(ctx: *mut c_void),
    callback_ctx: *mut c_void,
) -> *mut BtRusshClient {
    match BtRusshClient::new(Some(output_sink), Some(close_sink), callback_ctx) {
        Ok(client) => Box::into_raw(Box::new(client)),
        Err(_) => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn bt_russh_client_connect(
    client: *mut BtRusshClient,
    request: *const BtRusshConnectRequest,
) -> BtRusshResult {
    if client.is_null() {
        return result_from_error(ClientError::MissingClient);
    }
    let request = match read_connect_request(request) {
        Ok(request) => request,
        Err(error) => return result_from_error(error),
    };
    match (&*client).connect_blocking(request) {
        Ok(()) => ok(),
        Err(error) => result_from_error(error),
    }
}

#[no_mangle]
pub unsafe extern "C" fn bt_russh_client_write(
    client: *mut BtRusshClient,
    bytes: *const u8,
    len: usize,
) -> BtRusshResult {
    if client.is_null() {
        return result_from_error(ClientError::MissingClient);
    }
    if bytes.is_null() || len == 0 {
        return ok();
    }
    let bytes = std::slice::from_raw_parts(bytes, len);
    match (&*client).write_blocking(bytes) {
        Ok(()) => ok(),
        Err(error) => result_from_error(error),
    }
}

#[no_mangle]
pub unsafe extern "C" fn bt_russh_client_resize(
    client: *mut BtRusshClient,
    cols: u16,
    rows: u16,
) -> BtRusshResult {
    if client.is_null() {
        return result_from_error(ClientError::MissingClient);
    }
    match (&*client).resize_blocking(cols.max(1), rows.max(1)) {
        Ok(()) => ok(),
        Err(error) => result_from_error(error),
    }
}

#[no_mangle]
pub unsafe extern "C" fn bt_russh_client_disconnect(client: *mut BtRusshClient) -> BtRusshResult {
    if client.is_null() {
        return ok();
    }
    match (&*client).disconnect_blocking() {
        Ok(()) => ok(),
        Err(error) => result_from_error(error),
    }
}

#[no_mangle]
pub unsafe extern "C" fn bt_russh_client_release(client: *mut BtRusshClient) {
    if client.is_null() {
        return;
    }
    let client = Box::from_raw(client);
    let _ = client.disconnect_blocking();
}

#[no_mangle]
pub unsafe extern "C" fn bt_russh_result_message_free(message: *mut c_char) {
    if message.is_null() {
        return;
    }
    let _ = CString::from_raw(message);
}
