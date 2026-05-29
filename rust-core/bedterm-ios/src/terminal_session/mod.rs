//! Higher-level terminal session FFI: combines SSH lifecycle (connect →
//! open_shell → byte pump → disconnect) into a single handle-based
//! abstraction for Swift to drive.
//!
//! # Architecture
//!
//! - [`BtTerminalSessionHandle`] owns a single-threaded tokio runtime +
//!   `RusshSshClient` behind a `Mutex` + a background read-loop OS thread.
//! - Swift calls [`bt_terminal_session_create`] once, then
//!   [`bt_terminal_session_connect`] to open the session in one step
//!   (connect + open_shell + optional bootstrap payload + start read loop).
//! - State transitions are reported via a [`BtSessionStateCallback`] so
//!   Swift can update UI without polling.
//! - Completion callbacks (connect, send, resize) are delivered on the
//!   calling thread — Swift should dispatch to the main queue if needed.
//!
//! # Safety
//!
//! Every FFI function documents its safety preconditions. In general:
//! - `handle` must be a non-null pointer returned by
//!   [`bt_terminal_session_create`] that has not yet been passed to
//!   [`bt_terminal_session_close`].
//! - String parameters must be valid UTF-8 null-terminated C strings
//!   borrowed for the duration of the call.
//! - Callback function pointers must remain valid until the handle is
//!   closed.
//! - The `state_ctx` / `sink_ctx` / `completion_ctx` pointers must remain
//!   valid for the lifetime of the callbacks.

#![cfg(target_os = "ios")]

use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::ssh_bridge::{BtSSHCompletion, BtSSHOutputSink, BtSSHResultCode};
use crate::ssh_client::mock_impl::MockSshClient;
use crate::ssh_client::russh_impl::RusshSshClient;
use crate::ssh_client::{HostCredential, PtyDimensions, SshClient, SshConnectionRequest, SshError};

// ---------------------------------------------------------------------------
// AnyClient — dispatch enum over real and mock SSH clients
// ---------------------------------------------------------------------------

/// Holds either a production `RusshSshClient` or an in-process `MockSshClient`.
///
/// Exists so `TerminalSessionInner.client` can be typed without boxing, while
/// the read/write/resize/close operations dispatch to the correct impl.
enum AnyClient {
    Real(RusshSshClient),
    Mock(MockSshClient),
}

impl AnyClient {
    async fn read(&mut self) -> Result<Vec<u8>, SshError> {
        match self {
            AnyClient::Real(client) => client.read().await,
            AnyClient::Mock(client) => client.read().await,
        }
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), SshError> {
        match self {
            AnyClient::Real(client) => client.write(data).await,
            AnyClient::Mock(client) => client.write(data).await,
        }
    }

    async fn resize(&mut self, dims: PtyDimensions) -> Result<(), SshError> {
        match self {
            AnyClient::Real(client) => client.resize(dims).await,
            AnyClient::Mock(client) => client.resize(dims).await,
        }
    }

    async fn close(self) -> Result<(), SshError> {
        match self {
            AnyClient::Real(client) => client.close().await,
            AnyClient::Mock(client) => client.close().await,
        }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Poll interval for the background read loop (milliseconds). The read
/// thread wakes up this often to check the stop flag even when no data
/// has arrived.
const READ_POLL_MS: u64 = 200;

// ---------------------------------------------------------------------------
// Opaque context pointer newtype
// ---------------------------------------------------------------------------

/// Opaque context pointer for callbacks. Swift guarantees the pointed-to
/// object outlives the handle, so it is safe to send across thread
/// boundaries.
#[derive(Copy, Clone)]
struct SinkContext(*mut c_void);

// SAFETY: Swift guarantees the context pointer outlives the callbacks.
unsafe impl Send for SinkContext {}

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

/// Session lifecycle state. Transitions are always monotone:
/// Idle → Connecting → Open → Closed.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BtSessionState {
    Idle = 0,
    Connecting = 1,
    Open = 2,
    Closed = 3,
}

/// State-change notification fired when the session transitions state.
///
/// For the `Closed` transition, `error_code` and `exit_code` carry the
/// cause (`BtSSHResultOk` = caller-initiated disconnect, other codes
/// indicate errors). Always called on the thread that caused the
/// transition — Swift should dispatch to the main queue if needed.
pub type BtSessionStateCallback = unsafe extern "C" fn(
    ctx: *mut c_void,
    state: BtSessionState,
    error_code: BtSSHResultCode,
    exit_code: i32,
);

// ---------------------------------------------------------------------------
// SessionInfo
// ---------------------------------------------------------------------------

struct SessionInfo {
    state: BtSessionState,
    error_code: BtSSHResultCode,
    exit_code: i32,
}

// ---------------------------------------------------------------------------
// Inner shared state
// ---------------------------------------------------------------------------

struct TerminalSessionInner {
    /// Dedicated single-threaded tokio runtime for SSH operations.
    runtime: tokio::runtime::Runtime,
    /// The SSH client (`None` before connect or after close).
    client: Mutex<Option<AnyClient>>,
    /// Data sink installed by Swift for inbound PTY bytes.
    data_sink: Mutex<Option<(BtSSHOutputSink, SinkContext)>>,
    /// State-change callback installed at creation time.
    state_cb: Mutex<Option<(BtSessionStateCallback, SinkContext)>>,
    /// Current lifecycle state plus last error/exit info.
    session_info: Mutex<SessionInfo>,
    /// Signal to stop the background read-loop thread.
    stop_flag: AtomicBool,
}

// ---------------------------------------------------------------------------
// Opaque handle
// ---------------------------------------------------------------------------

/// Opaque handle owning a running SSH terminal session.
///
/// Created by [`bt_terminal_session_create`], freed by
/// [`bt_terminal_session_close`].
pub struct BtTerminalSessionHandle {
    inner: Arc<TerminalSessionInner>,
    /// Background OS thread running the read loop (if spawned).
    read_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Mock script name installed by [`bt_terminal_session_install_mock`].
    ///
    /// When non-`None`, [`bt_terminal_session_connect`] bypasses real SSH
    /// and constructs a [`MockSshClient`] instead. The value is consumed
    /// (set back to `None`) on the first connect so a reconnect attempt
    /// uses the real SSH path.
    mock_script_override: Mutex<Option<String>>,
}

// ---------------------------------------------------------------------------
// Helper functions (private, mirrored from ssh_client_ffi.rs)
// ---------------------------------------------------------------------------

/// Unwrap a `std::sync::Mutex`, recovering from poison.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Convert a C string pointer to a `&str`, returning `None` on null or
/// invalid UTF-8.
fn cstr<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

/// Call a [`BtSSHCompletion`] callback with a `Result<(), SshError>`.
///
/// # Safety
///
/// `completion` must be a valid function pointer. `ctx` is forwarded
/// verbatim to the callback.
unsafe fn call_completion(
    completion: BtSSHCompletion,
    ctx: *mut c_void,
    result: Result<(), SshError>,
) {
    let (code, extra) = match result {
        Ok(()) => (BtSSHResultCode::BtSSHResultOk, 0),
        Err(ref err) => {
            let extra = if let SshError::ShellExited(code) = err {
                *code
            } else {
                0
            };
            (err.result_code(), extra)
        }
    };
    completion(ctx, code, std::ptr::null(), extra);
}

/// Parse a JSON credential string into a [`HostCredential`].
///
/// Expected JSON shapes (selected by `"type"` discriminator):
///
/// ```json
/// {"type": "password", "password": "…"}
/// {"type": "private_key", "private_key": "…", "passphrase": "…"}
/// {"type": "agent"}
/// ```
fn parse_credential(json: &str) -> Result<HostCredential, String> {
    #[derive(serde::Deserialize)]
    #[serde(tag = "type")]
    enum JsonCredential {
        #[serde(rename = "password")]
        Password { password: String },
        #[serde(rename = "private_key")]
        PrivateKey {
            private_key: String,
            passphrase: Option<String>,
        },
        #[serde(rename = "agent")]
        Agent,
    }

    let jc: JsonCredential =
        serde_json::from_str(json).map_err(|e| format!("invalid credential JSON: {e}"))?;
    Ok(match jc {
        JsonCredential::Password { password } => HostCredential::Password { password },
        JsonCredential::PrivateKey {
            private_key,
            passphrase,
        } => HostCredential::PrivateKey {
            private_key: private_key.into_bytes(),
            passphrase,
        },
        JsonCredential::Agent => HostCredential::Agent,
    })
}

// ---------------------------------------------------------------------------
// State transition helper
// ---------------------------------------------------------------------------

/// Update the session state and fire the state-change callback.
fn transition_state(
    inner: &TerminalSessionInner,
    state: BtSessionState,
    error_code: BtSSHResultCode,
    exit_code: i32,
) {
    {
        let mut info = lock(&inner.session_info);
        info.state = state;
        info.error_code = error_code;
        info.exit_code = exit_code;
    }
    let cb = lock(&inner.state_cb);
    if let Some((callback, ctx)) = *cb {
        // SAFETY: `callback` is a valid function pointer installed by Swift.
        // `ctx` is valid for the lifetime of the handle (Swift contract).
        unsafe { callback(ctx.0, state, error_code, exit_code) };
    }
}

// ---------------------------------------------------------------------------
// Background read loop
// ---------------------------------------------------------------------------

/// Background read-loop thread entry point.
///
/// Repeatedly calls `client.read()` (with a poll timeout so the thread
/// can check the stop flag), and pushes received data to the Swift-side
/// data sink. On EOF or error, transitions state to Closed and fires the
/// state callback.
fn session_read_loop(inner: Arc<TerminalSessionInner>) {
    loop {
        // 1. Check stop signal before trying to lock anything.
        if inner.stop_flag.load(Ordering::SeqCst) {
            break;
        }

        // 2. Try to read a chunk (with timeout so we can re-check the flag).
        let read_result: Option<Result<Vec<u8>, SshError>> = {
            let mut guard = lock(&inner.client);
            match guard.as_mut() {
                Some(client) => {
                    let timed = inner.runtime.block_on(async {
                        tokio::time::timeout(Duration::from_millis(READ_POLL_MS), client.read())
                            .await
                    });
                    match timed {
                        Ok(result) => Some(result),
                        Err(_elapsed) => None, // timeout — loop back and re-check stop flag
                    }
                }
                None => break, // client was taken (close in progress)
            }
        };

        // 3. Process the result.
        match read_result {
            Some(Ok(data)) if !data.is_empty() => {
                let cb = lock(&inner.data_sink);
                if let Some((sink, sink_ctx)) = *cb {
                    // SAFETY: `sink` is a valid function pointer installed by Swift.
                    // `sink_ctx` is valid because close() joins this thread before
                    // the handle is freed.
                    unsafe { sink(sink_ctx.0, data.as_ptr(), data.len()) };
                }
            }
            Some(Ok(_)) => {
                // EOF — empty vec means channel closed normally.
                transition_state(
                    &inner,
                    BtSessionState::Closed,
                    BtSSHResultCode::BtSSHResultDisconnected,
                    0,
                );
                break;
            }
            Some(Err(err)) => {
                let (code, exit) = match &err {
                    SshError::ShellExited(c) => (BtSSHResultCode::BtSSHResultShellExited, *c),
                    _ => (err.result_code(), 0),
                };
                transition_state(&inner, BtSessionState::Closed, code, exit);
                break;
            }
            None => continue, // timeout — re-check stop flag
        }
    }
}

// ---------------------------------------------------------------------------
// try_start_read_loop
// ---------------------------------------------------------------------------

/// Spawn the background read-loop thread if (and only if) a data sink is
/// installed, the client is present, and no read thread is already
/// running.
///
/// Safe to call multiple times — subsequent calls are no-ops.
fn try_start_read_loop(handle: &BtTerminalSessionHandle) {
    let has_sink = lock(&handle.inner.data_sink).is_some();
    let has_client = lock(&handle.inner.client).is_some();
    let already_running = lock(&handle.read_thread).is_some();

    if has_sink && has_client && !already_running {
        let inner = Arc::clone(&handle.inner);
        let builder_result = std::thread::Builder::new()
            .name("bt-session-read-loop".into())
            .spawn(move || {
                session_read_loop(inner);
            });
        match builder_result {
            Ok(thread) => {
                *lock(&handle.read_thread) = Some(thread);
            }
            Err(_) => {
                // Failed to spawn the read-loop OS thread. The Swift side
                // will not receive data callbacks; it can detect this by
                // timing out or observing no state change.
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FFI exports
// ---------------------------------------------------------------------------

/// Create a new terminal session handle.
///
/// Spawns a single-threaded tokio runtime and returns an opaque handle
/// ready for [`bt_terminal_session_connect`].
///
/// Returns NULL if the tokio runtime could not be created.
///
/// # Parameters
///
/// - `state_cb`: nullable state-change callback. Fired on each lifecycle
///   transition (Connecting → Open → Closed).
/// - `state_ctx`: opaque context threaded verbatim to `state_cb`.
///
/// # Safety
///
/// `state_cb` (if non-null) must be a valid function pointer valid for the
/// lifetime of the handle. `state_ctx` must remain valid for the same
/// duration.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_create(
    // Inline the bare-fn type rather than `Option<BtSessionStateCallback>` so
    // cbindgen emits a nullable C function pointer rather than an Option_ struct.
    state_cb: Option<
        unsafe extern "C" fn(
            ctx: *mut c_void,
            state: BtSessionState,
            error_code: BtSSHResultCode,
            exit_code: i32,
        ),
    >,
    state_ctx: *mut c_void,
) -> *mut BtTerminalSessionHandle {
    let result = std::panic::catch_unwind(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();

        match runtime {
            Ok(runtime) => {
                let state_cb_entry = state_cb.map(|cb| {
                    // Cast the inline fn type to the BtSessionStateCallback alias.
                    let cb: BtSessionStateCallback = cb;
                    (cb, SinkContext(state_ctx))
                });
                let handle = BtTerminalSessionHandle {
                    inner: Arc::new(TerminalSessionInner {
                        runtime,
                        client: Mutex::new(None),
                        data_sink: Mutex::new(None),
                        state_cb: Mutex::new(state_cb_entry),
                        session_info: Mutex::new(SessionInfo {
                            state: BtSessionState::Idle,
                            error_code: BtSSHResultCode::BtSSHResultOk,
                            exit_code: 0,
                        }),
                        stop_flag: AtomicBool::new(false),
                    }),
                    read_thread: Mutex::new(None),
                    mock_script_override: Mutex::new(None),
                };
                Box::into_raw(Box::new(handle))
            }
            Err(_) => std::ptr::null_mut(),
        }
    });

    result.unwrap_or(std::ptr::null_mut())
}

/// Connect to an SSH server, open a PTY shell, and optionally send a
/// bootstrap payload — all in one blocking call on the calling thread.
///
/// On success: stores the client, transitions state → Open, fires
/// `state_cb`, starts the background read loop (if a data sink is
/// installed), then calls `completion(Ok)`.
///
/// On error: transitions state → Closed with the appropriate error code,
/// then calls `completion(Err)`.
///
/// # Parameters
///
/// - `handle`: opaque handle from [`bt_terminal_session_create`].
/// - `host`: UTF-8 C string, remote hostname or IP. Must not be NULL.
/// - `port`: TCP port (typically 22).
/// - `username`: UTF-8 C string, SSH username. Must not be NULL.
/// - `credential_json`: UTF-8 C string JSON describing the credential.
///   Must not be NULL.
/// - `bootstrap_payload`: nullable UTF-8 C string sent to the shell
///   immediately after opening (e.g. a shell integration script).
/// - `cols` / `rows`: initial PTY dimensions in character cells.
/// - `timeout_ms`: if > 0, the entire connect + open_shell sequence is
///   wrapped in a timeout. 0 means no timeout.
/// - `completion`: callback invoked with the connection result.
/// - `completion_ctx`: opaque context for `completion`.
///
/// # Safety
///
/// See the module-level safety documentation.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bt_terminal_session_connect(
    handle: *mut BtTerminalSessionHandle,
    host: *const c_char,
    port: u16,
    username: *const c_char,
    credential_json: *const c_char,
    bootstrap_payload: *const c_char,
    cols: u16,
    rows: u16,
    timeout_ms: u64,
    completion: BtSSHCompletion,
    completion_ctx: *mut c_void,
) {
    let result = std::panic::catch_unwind(|| {
        let handle = if handle.is_null() {
            call_completion(
                completion,
                completion_ctx,
                Err(SshError::Other("null handle".into())),
            );
            return;
        } else {
            &*handle
        };

        // ── Parse string parameters ────────────────────────────────────────

        let host_str = match cstr(host) {
            Some(s) => s,
            None => {
                call_completion(
                    completion,
                    completion_ctx,
                    Err(SshError::Other("null host".into())),
                );
                return;
            }
        };
        let username_str = match cstr(username) {
            Some(s) => s,
            None => {
                call_completion(
                    completion,
                    completion_ctx,
                    Err(SshError::Other("null username".into())),
                );
                return;
            }
        };
        let credential_json_str = match cstr(credential_json) {
            Some(s) => s,
            None => {
                call_completion(
                    completion,
                    completion_ctx,
                    Err(SshError::Other("null credential_json".into())),
                );
                return;
            }
        };
        // bootstrap_payload is nullable — copy to owned String now so it
        // can cross the async boundary.
        let bootstrap_bytes: Option<Vec<u8>> = cstr(bootstrap_payload)
            .filter(|s| !s.is_empty())
            .map(|s| s.as_bytes().to_vec());

        // ── Parse credential ───────────────────────────────────────────────

        let credential = match parse_credential(credential_json_str) {
            Ok(c) => c,
            Err(msg) => {
                call_completion(completion, completion_ctx, Err(SshError::Other(msg)));
                return;
            }
        };

        // ── Build connection request ───────────────────────────────────────

        let request = SshConnectionRequest {
            host: host_str.to_string(),
            port,
            username: username_str.to_string(),
            credential,
            proxy_command: None,
        };

        let dims = PtyDimensions::new(cols, rows);

        // ── Transition → Connecting ────────────────────────────────────────

        transition_state(
            &handle.inner,
            BtSessionState::Connecting,
            BtSSHResultCode::BtSSHResultOk,
            0,
        );

        // ── Check for mock override ────────────────────────────────────────
        //
        // Consume the mock-script override (if set) so that a subsequent
        // reconnect attempt falls through to the real SSH path.
        let mock_script = { lock(&handle.mock_script_override).take() };

        // ── Connect + open_shell (blocking) ───────────────────────────────

        let connect_result: Result<AnyClient, SshError> = if let Some(script_name) = mock_script {
            // Mock path: skip real SSH, construct the mock directly.
            handle.inner.runtime.block_on(async {
                let mut mock_client = MockSshClient::from_script(&script_name);
                mock_client.open_shell(dims).await?;
                if let Some(ref payload) = bootstrap_bytes {
                    // write() in non-echo mode is a no-op, so this is safe.
                    mock_client.write(payload).await?;
                }
                Ok::<AnyClient, SshError>(AnyClient::Mock(mock_client))
            })
        } else {
            // Real SSH path.
            handle.inner.runtime.block_on(async {
                let connect_fut = async {
                    let mut client = RusshSshClient::connect(request).await?;
                    client.open_shell(dims).await?;
                    if let Some(payload) = bootstrap_bytes {
                        client.write(&payload).await?;
                    }
                    Ok::<AnyClient, SshError>(AnyClient::Real(client))
                };

                if timeout_ms > 0 {
                    tokio::time::timeout(Duration::from_millis(timeout_ms), connect_fut)
                        .await
                        .unwrap_or(Err(SshError::Timeout))
                } else {
                    connect_fut.await
                }
            })
        };

        // ── Handle result ─────────────────────────────────────────────────

        match connect_result {
            Ok(client) => {
                *lock(&handle.inner.client) = Some(client);
                transition_state(
                    &handle.inner,
                    BtSessionState::Open,
                    BtSSHResultCode::BtSSHResultOk,
                    0,
                );
                try_start_read_loop(handle);
                call_completion(completion, completion_ctx, Ok(()));
            }
            Err(e) => {
                let (code, exit) = match &e {
                    SshError::ShellExited(c) => (BtSSHResultCode::BtSSHResultShellExited, *c),
                    _ => (e.result_code(), 0),
                };
                transition_state(&handle.inner, BtSessionState::Closed, code, exit);
                call_completion(completion, completion_ctx, Err(e));
            }
        }
    });

    if result.is_err() {
        call_completion(
            completion,
            completion_ctx,
            Err(SshError::Other("Rust panic during connect".into())),
        );
    }
}

/// Install (or replace) the data sink for inbound PTY bytes.
///
/// If the shell is already open and no read thread is running, calling
/// this function automatically spawns the background read loop.
///
/// # Parameters
///
/// - `handle`: opaque handle.
/// - `sink`: callback function pointer. Must be non-null.
/// - `sink_ctx`: opaque context threaded verbatim to `sink`.
///
/// # Safety
///
/// `sink` must be a valid function pointer valid for the lifetime of the
/// handle or until [`bt_terminal_session_close`] is called. `sink_ctx`
/// must remain valid for the same duration.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_set_data_sink(
    handle: *mut BtTerminalSessionHandle,
    sink: BtSSHOutputSink,
    sink_ctx: *mut c_void,
) {
    let _ = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }
        let handle = &*handle;

        {
            let mut ds = lock(&handle.inner.data_sink);
            *ds = Some((sink, SinkContext(sink_ctx)));
        }

        // If the shell is already open, try to start the read loop now.
        try_start_read_loop(handle);
    });
}

/// Write bytes to the SSH channel (stdin of the remote shell).
///
/// The completion callback is invoked on the *calling* thread after the
/// write completes (or fails).
///
/// # Parameters
///
/// - `handle`: opaque handle, shell must be open.
/// - `bytes`: pointer to `len` bytes of data.
/// - `len`: number of bytes to write.
/// - `completion`: callback invoked with the result.
/// - `completion_ctx`: opaque context for `completion`.
///
/// # Safety
///
/// `bytes` must be valid for `len` bytes. Passing NULL with len > 0 is
/// undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_send(
    handle: *mut BtTerminalSessionHandle,
    bytes: *const u8,
    len: usize,
    completion: BtSSHCompletion,
    completion_ctx: *mut c_void,
) {
    let result = std::panic::catch_unwind(|| {
        let handle = if handle.is_null() {
            call_completion(
                completion,
                completion_ctx,
                Err(SshError::Other("null handle".into())),
            );
            return;
        } else {
            &*handle
        };

        let data: &[u8] = if len == 0 || bytes.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(bytes, len) }
        };

        let mut guard = lock(&handle.inner.client);
        let client = match guard.as_mut() {
            Some(c) => c,
            None => {
                call_completion(
                    completion,
                    completion_ctx,
                    Err(SshError::Other("shell not open".into())),
                );
                return;
            }
        };

        let result = handle.inner.runtime.block_on(client.write(data));

        match result {
            Ok(()) => call_completion(completion, completion_ctx, Ok(())),
            Err(e) => call_completion(completion, completion_ctx, Err(e)),
        }
    });

    if result.is_err() {
        call_completion(
            completion,
            completion_ctx,
            Err(SshError::Other("Rust panic during send".into())),
        );
    }
}

/// Resize the remote PTY.
///
/// The completion callback is invoked on the *calling* thread after the
/// resize completes (or fails).
///
/// # Parameters
///
/// - `handle`: opaque handle, shell must be open.
/// - `cols`: new terminal width in character cells.
/// - `rows`: new terminal height in character cells.
/// - `completion`: callback invoked with the result.
/// - `completion_ctx`: opaque context for `completion`.
///
/// # Safety
///
/// See the module-level safety documentation.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_resize(
    handle: *mut BtTerminalSessionHandle,
    cols: u16,
    rows: u16,
    completion: BtSSHCompletion,
    completion_ctx: *mut c_void,
) {
    let result = std::panic::catch_unwind(|| {
        let handle = if handle.is_null() {
            call_completion(
                completion,
                completion_ctx,
                Err(SshError::Other("null handle".into())),
            );
            return;
        } else {
            &*handle
        };

        let dims = PtyDimensions::new(cols, rows);
        let mut guard = lock(&handle.inner.client);
        let client = match guard.as_mut() {
            Some(c) => c,
            None => {
                call_completion(
                    completion,
                    completion_ctx,
                    Err(SshError::Other("shell not open".into())),
                );
                return;
            }
        };

        let result = handle.inner.runtime.block_on(client.resize(dims));

        match result {
            Ok(()) => call_completion(completion, completion_ctx, Ok(())),
            Err(e) => call_completion(completion, completion_ctx, Err(e)),
        }
    });

    if result.is_err() {
        call_completion(
            completion,
            completion_ctx,
            Err(SshError::Other("Rust panic during resize".into())),
        );
    }
}

/// Initiate a non-blocking disconnect.
///
/// Sets the stop flag (so the read loop exits on its next wake), takes
/// the SSH client, closes it, and fires the state callback with
/// `Closed + BtSSHResultOk` to signal a caller-initiated disconnect.
///
/// This call does NOT join the read thread — use
/// [`bt_terminal_session_close`] to fully tear down the handle.
///
/// # Safety
///
/// See the module-level safety documentation.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_disconnect(handle: *mut BtTerminalSessionHandle) {
    let _ = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }
        let handle = &*handle;

        // 1. Signal the read loop to stop.
        handle.inner.stop_flag.store(true, Ordering::SeqCst);

        // 2. Take the client and close it immediately (don't wait for the
        //    read loop thread to observe the stop flag).
        let client = { lock(&handle.inner.client).take() };
        if let Some(client) = client {
            let _ = handle.inner.runtime.block_on(client.close());
        }

        // 3. Transition → Closed with Ok (caller-initiated).
        transition_state(
            &handle.inner,
            BtSessionState::Closed,
            BtSSHResultCode::BtSSHResultOk,
            0,
        );
    });
}

/// Fully tear down the session handle and free all resources.
///
/// Signals the background read loop to stop, joins the read-loop thread,
/// takes and closes the SSH client, then deallocates the handle.
///
/// After this call the handle pointer is invalid and must not be used
/// again. Safe to call with NULL.
///
/// # Safety
///
/// `handle` must be non-NULL and returned by [`bt_terminal_session_create`]
/// that has not already been passed to `bt_terminal_session_close`.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_close(handle: *mut BtTerminalSessionHandle) {
    let _ = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }

        // Take ownership of the box — it drops when we're done.
        let handle = unsafe { Box::from_raw(handle) };

        // 1. Signal the read loop to stop.
        handle.inner.stop_flag.store(true, Ordering::SeqCst);

        // 2. Join the background read thread (if any). The thread checks
        //    the stop flag at most READ_POLL_MS after signalling.
        if let Some(thread) = lock(&handle.read_thread).take() {
            let _ = thread.join();
        }

        // 3. Take the client and close it.
        let client = { lock(&handle.inner.client).take() };
        if let Some(client) = client {
            let _ = handle.inner.runtime.block_on(client.close());
        }

        // 4. `handle` (the Box) drops here, which drops the
        //    `Arc<TerminalSessionInner>`, and the tokio runtime drops last.
    });
}

/// Return the current session lifecycle state.
///
/// # Safety
///
/// `handle` must be non-null and valid (not yet passed to
/// [`bt_terminal_session_close`]).
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_state(
    handle: *const BtTerminalSessionHandle,
) -> BtSessionState {
    let result = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return BtSessionState::Closed;
        }
        let handle = &*handle;
        lock(&handle.inner.session_info).state
    });
    result.unwrap_or(BtSessionState::Closed)
}

/// Return the error code from the last session close (or `BtSSHResultOk`
/// if the session closed normally / has not yet closed).
///
/// # Safety
///
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_last_error_code(
    handle: *const BtTerminalSessionHandle,
) -> BtSSHResultCode {
    let result = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return BtSSHResultCode::BtSSHResultOther;
        }
        let handle = &*handle;
        lock(&handle.inner.session_info).error_code
    });
    result.unwrap_or(BtSSHResultCode::BtSSHResultOther)
}

/// Return the shell exit code from the last session close (0 if not a
/// shell-exited close, or the session has not yet closed).
///
/// # Safety
///
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_last_exit_code(
    handle: *const BtTerminalSessionHandle,
) -> i32 {
    let result = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return -1;
        }
        let handle = &*handle;
        lock(&handle.inner.session_info).exit_code
    });
    result.unwrap_or(-1)
}

/// Install a named mock SSH client for UI testing.
///
/// Must be called **before** [`bt_terminal_session_connect`]. When a
/// non-empty script name is installed, the next connect bypasses real SSH
/// and uses a [`MockSshClient`] instead. The override is consumed on the
/// first connect so a subsequent reconnect uses the real SSH path.
///
/// Scripts (matching `UITestSupport.swift`):
///
/// | Name         | Behaviour                                             |
/// |--------------|-------------------------------------------------------|
/// | `"hello"`    | Emits `"Hello, world!\r\n"` then blocks.              |
/// | `"ansiColors"` | Emits ANSI red/green/blue sequence then blocks.     |
/// | `"prompt"`   | Emits `"bedterm$ "` then blocks.                      |
/// | `"echo"`     | Emits `"bedterm$ "`, echoes writes back as magenta.   |
///
/// NULL or empty `script` is a no-op.
///
/// # Safety
///
/// `handle` must be a live handle not yet connected.
/// `script` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_install_mock(
    handle: *mut BtTerminalSessionHandle,
    script: *const c_char,
) {
    let _ = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }
        let handle = &*handle;
        if let Some(script_str) = cstr(script) {
            if !script_str.is_empty() {
                *lock(&handle.mock_script_override) = Some(script_str.to_string());
            }
        }
    });
}
