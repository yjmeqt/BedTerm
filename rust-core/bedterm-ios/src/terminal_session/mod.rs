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

use bedterm_app::credential;
use bedterm_app::ssh_bridge::{BtSSHCompletion, BtSSHOutputSink, BtSSHResultCode};
use bedterm_app::ssh_client::russh_impl::RusshSshClient;
use bedterm_app::ssh_client::{PtyDimensions, SshClient, SshConnectionRequest, SshError};

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// AnyClient — newtype wrapper over the production SSH client
// ---------------------------------------------------------------------------

/// Wraps a production `RusshSshClient` so `TerminalSessionInner.client` can be
/// typed as `Option<AnyClient>` without boxing.
struct AnyClient(RusshSshClient);

impl AnyClient {
    async fn read(&mut self) -> Result<Vec<u8>, SshError> {
        self.0.read().await
    }

    #[allow(dead_code)]
    async fn write(&mut self, data: &[u8]) -> Result<(), SshError> {
        self.0.write(data).await
    }

    #[allow(dead_code)]
    async fn resize(&mut self, dims: PtyDimensions) -> Result<(), SshError> {
        self.0.resize(dims).await
    }

    async fn close(self) -> Result<(), SshError> {
        self.0.close().await
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

/// Parse a JSON credential string into a [`credential::HostCredential`].
///
/// Uses the canonical [`credential::HostCredential`] serde definition for
/// the JSON schema instead of a local ad-hoc enum.
///
/// Expected JSON shapes (selected by `"type"` discriminator):
///
/// ```json
/// {"type": "password", "password": "…"}
/// {"type": "private_key", "private_key": "…", "passphrase": "…"}
/// {"type": "agent"}
/// ```
fn parse_credential(json: &str) -> Result<credential::HostCredential, String> {
    serde_json::from_str::<credential::HostCredential>(json)
        .map_err(|e| format!("invalid credential JSON: {e}"))
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

        // ── Parse credential ───────────────────────────────────────────────

        let canonical_cred = match parse_credential(credential_json_str) {
            Ok(c) => c,
            Err(msg) => {
                call_completion(completion, completion_ctx, Err(SshError::Other(msg)));
                return;
            }
        };

        // ── Convert to transport-level credential for the SSH trait ─────────

        let transport_cred = match &canonical_cred {
            credential::HostCredential::Password { password } => {
                bedterm_app::ssh_client::HostCredential::Password {
                    password: password.clone(),
                }
            }
            credential::HostCredential::Agent => bedterm_app::ssh_client::HostCredential::Agent,
        };

        // ── Build canonical connection request ────────────────────────────────

        let canonical_request = credential::SshConnectionRequest {
            credential: credential::ConnectionFields {
                host: host_str.to_string(),
                port,
                username: username_str.to_string(),
                credential: canonical_cred,
            },
            initial_pty: PtyDimensions::new(cols, rows),
            bootstrap_payload: cstr(bootstrap_payload)
                .filter(|s| !s.is_empty())
                .map(String::from),
        };

        // ── Build transport request from canonical fields ─────────────────────

        let request = SshConnectionRequest {
            host: canonical_request.credential.host.clone(),
            port: canonical_request.credential.port,
            username: canonical_request.credential.username.clone(),
            credential: transport_cred,
            proxy_command: None,
        };

        let dims = PtyDimensions::new(cols, rows);
        let bs = canonical_request.bootstrap_payload.clone();

        // ── Transition → Connecting ────────────────────────────────────────

        transition_state(
            &handle.inner,
            BtSessionState::Connecting,
            BtSSHResultCode::BtSSHResultOk,
            0,
        );

        // ── Connect + open_shell (blocking) ───────────────────────────────

        let connect_result: Result<AnyClient, SshError> = handle.inner.runtime.block_on(async {
            let connect_fut = async {
                let mut client = RusshSshClient::connect(request).await?;
                client.open_shell(dims).await?;
                if let Some(ref payload) = bs {
                    client.write(payload.as_bytes()).await?;
                }
                Ok::<AnyClient, SshError>(AnyClient(client))
            };

            if timeout_ms > 0 {
                tokio::time::timeout(Duration::from_millis(timeout_ms), connect_fut)
                    .await
                    .unwrap_or(Err(SshError::Timeout))
            } else {
                connect_fut.await
            }
        });

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

/// Replaces the per-session data sink with a direct feed into the given
/// `BtIosMetalInputView`. After this call, inbound PTY bytes are pushed
/// straight into the metal view's `BtTerm` renderer — no Swift pump task,
/// no `AsyncStream`, no C-callback trampoline through Swift.
///
/// This is the Phase 3 bridge: the Swift `RustTerminalSession` pump task
/// (`startPumpTask`) and the `IosTerminalHost` feed-forwarding task can
/// both be retired once every session calls this function.
///
/// # Safety
///
/// `handle` must be a valid non-null pointer returned by
/// [`bt_terminal_session_create`]. `metal_view_ptr` must point to a live
/// `BtIosMetalInputView` that outlives the session. Must be called on the
/// main thread (the metal view is a UIKit object).
#[no_mangle]
pub unsafe extern "C" fn bt_terminal_session_attach_metal_view(
    handle: *mut BtTerminalSessionHandle,
    metal_view_ptr: *mut c_void,
) {
    // Null metal_view_ptr is a no-op — callers can use it to detach.
    if metal_view_ptr.is_null() {
        return;
    }
    // Safety: bt_ios_view_feed_bytes does its own null checks and is
    // main-thread-only, same as the caller contract above.
    let sink: BtSSHOutputSink = metal_view_sink_trampoline;
    // The ctx IS the metal view pointer — no indirection.
    let ctx = metal_view_ptr;
    unsafe {
        bt_terminal_session_set_data_sink(handle, sink, ctx);
    }
}

/// Trampoline from `BtSSHOutputSink` signature to `bt_ios_view_feed_bytes`.
/// The `ctx` parameter is the `BtIosMetalInputView *` raw pointer.
unsafe extern "C" fn metal_view_sink_trampoline(ctx: *mut c_void, bytes: *const u8, len: usize) {
    // Forward to the shared FFI function which does its own null checks.
    crate::ffi::view::bt_ios_view_feed_bytes(ctx, bytes, len);
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
