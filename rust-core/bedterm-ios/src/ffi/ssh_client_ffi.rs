//! C FFI layer exporting the Rust `RusshSshClient` for Swift to call.
//!
//! This is the OPPOSITE direction from [`crate::ssh_bridge`] (which lets Rust
//! call Swift's Citadel client).  Here Rust owns the SSH client entirely and
//! Swift drives it through handle-based FFI functions.
//!
//! # Architecture
//!
//! - [`SshClientHandle`] packs a single-threaded tokio runtime + the
//!   `RusshSshClient` behind a `Mutex` + a background read-loop OS thread.
//! - Swift creates the handle once, then calls the `bt_ssh_client_*` functions
//!   to drive the lifecycle: connect → open_shell → read/write/resize → close.
//! - Fast operations (write, resize) use [`Runtime::block_on`] synchronously.
//! - The background read loop calls `client.read()` with a short poll timeout
//!   and pushes received data to the Swift-supplied [`BtSSHOutputSink`].
//! - Completion callbacks (connect, open_shell, write, resize, close) are
//!   delivered on the calling thread — the Swift side should dispatch to the
//!   main queue if needed.
//! - All panics are caught at the FFI boundary.
//!
//! # Safety
//!
//! Every FFI function documents its safety preconditions.  In general:
//! - `handle` must be a non-null pointer returned by [`bt_ssh_client_create`]
//!   that has not yet been passed to [`bt_ssh_client_close`].
//! - String parameters must be valid UTF-8 null-terminated C strings borrowed
//!   for the duration of the call.
//! - Completion callbacks must be valid function pointers.
//! - The read-callback `sink_ctx` must remain valid until the callback is
//!   unset or the handle is closed.

#![cfg(target_os = "ios")]

use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Opaque context pointer for the read callback. Swift guarantees the
/// pointed-to object outlives the read-loop thread, so it is safe to
/// send across thread boundaries.
#[derive(Copy, Clone)]
struct SinkContext(*mut c_void);
unsafe impl Send for SinkContext {}

use crate::ssh_bridge::{BtSSHCompletion, BtSSHOutputSink, BtSSHResultCode};
use crate::ssh_client::russh_impl::RusshSshClient;
use crate::ssh_client::{HostCredential, PtyDimensions, SshClient, SshConnectionRequest, SshError};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Poll interval for the background read loop (milliseconds).  The read
/// thread wakes up this often to check the stop flag even when no data
/// has arrived.
const READ_POLL_MS: u64 = 200;

// ---------------------------------------------------------------------------
// Helper functions
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

/// Map an [`SshError`] to [`BtSSHResultCode`].
fn result_code(err: &SshError) -> BtSSHResultCode {
    err.result_code()
}

/// Call a [`BtSSHCompletion`] callback with a `Result<(), SshError>`.
///
/// The `msg` parameter is left NULL — Swift infers error context from the
/// result code and the optional exit code (for `ShellExited`).
///
/// # Safety
///
/// `completion` must be a valid function pointer.  `ctx` is forwarded
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
            (result_code(err), extra)
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
// Background read loop
// ---------------------------------------------------------------------------

/// Internal state behind an `Arc` so the read-loop thread can share it
/// with the owning handle.
struct SshClientInner {
    /// Dedicated single-threaded tokio runtime for SSH operations.
    runtime: tokio::runtime::Runtime,
    /// The SSH client (`None` after close or before connect).
    client: Mutex<Option<RusshSshClient>>,
    /// Read callback installed by Swift.
    read_callback: Mutex<Option<(BtSSHOutputSink, SinkContext)>>,
    /// Signal to stop the background read-loop thread.
    stop_read_loop: AtomicBool,
}

/// Background read-loop thread entry point.
///
/// Repeatedly calls `client.read()` (with a poll timeout so the thread
/// can check the stop flag), and pushes received data to the Swift-side
/// read callback.  Exits on EOF, error, or stop signal.
fn read_loop(inner: Arc<SshClientInner>) {
    loop {
        // 1. Check stop signal before trying to lock anything.
        if inner.stop_read_loop.load(Ordering::SeqCst) {
            break;
        }

        // 2. Try to read a chunk (with timeout).
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
                        Err(_elapsed) => None, // timeout — loop back and
                                               // re-check the stop flag
                    }
                }
                None => break, // client was taken (close in progress)
            }
        };

        // 3. Process the result.
        match read_result {
            Some(Ok(data)) if !data.is_empty() => {
                let cb = lock(&inner.read_callback);
                if let Some((sink, sink_ctx)) = *cb {
                    // SAFETY: `sink` is a valid function pointer installed
                    // by Swift.  `sink_ctx` is valid because close() joins
                    // this thread before the handle is freed and because
                    // unsetting the callback would require this same lock.
                    unsafe { sink(sink_ctx.0, data.as_ptr(), data.len()) };
                }
            }
            Some(Ok(_)) => break,  // EOF — empty vec means channel closed
            Some(Err(_)) => break, // error — connection lost
            None => continue,      // timeout — re-check stop flag
        }
    }
}

// ---------------------------------------------------------------------------
// Opaque handle
// ---------------------------------------------------------------------------

/// Opaque handle owning a running SSH client + tokio runtime + background
/// read-loop thread.
///
/// Created by [`bt_ssh_client_create`], freed by [`bt_ssh_client_close`].
pub struct SshClientHandle {
    inner: Arc<SshClientInner>,
    /// Background OS thread running the read loop (if spawned).
    read_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

// ---------------------------------------------------------------------------
// FFI exports
// ---------------------------------------------------------------------------

/// Create a new SSH client handle.
///
/// Spawns a single-threaded tokio runtime and returns an opaque handle.
/// The handle is ready for [`bt_ssh_client_connect`].
///
/// Returns NULL if the tokio runtime could not be created.
#[no_mangle]
pub extern "C" fn bt_ssh_client_create() -> *mut SshClientHandle {
    let result = std::panic::catch_unwind(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();

        match runtime {
            Ok(runtime) => {
                let handle = SshClientHandle {
                    inner: Arc::new(SshClientInner {
                        runtime,
                        client: Mutex::new(None),
                        read_callback: Mutex::new(None),
                        stop_read_loop: AtomicBool::new(false),
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

/// Connect to an SSH server.
///
/// Parses the connection parameters, calls
/// [`RusshSshClient::connect`](SshClient::connect) on the tokio runtime
/// (blocking the calling thread until complete), and stores the connected
/// client in the handle.
///
/// The completion callback is invoked on the *calling* thread.
///
/// # Parameters
///
/// - `handle`: opaque handle from [`bt_ssh_client_create`].
/// - `host`: UTF-8 C string, remote hostname or IP.  Must not be NULL.
/// - `port`: TCP port (typically 22).
/// - `username`: UTF-8 C string, SSH username.  Must not be NULL.
/// - `credential_json`: UTF-8 C string.  JSON describing a
///   [`HostCredential`] (see [`parse_credential`]).  Must not be NULL.
/// - `proxy_command`: UTF-8 C string or NULL for no proxy command.
/// - `completion`: callback invoked with the connection result.
/// - `completion_ctx`: opaque context threaded verbatim to `completion`.
///
/// # Safety
///
/// See the module-level safety documentation.  NULL string parameters
/// (other than `proxy_command`) cause a `BtSSHResultOther` completion.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bt_ssh_client_connect(
    handle: *mut SshClientHandle,
    host: *const c_char,
    port: u16,
    username: *const c_char,
    credential_json: *const c_char,
    proxy_command: *const c_char,
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

        // ── Parse string parameters ────────────────────────────────────

        let host = match cstr(host) {
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
        let username = match cstr(username) {
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
        let proxy_command = cstr(proxy_command); // nullable

        // ── Parse credential ───────────────────────────────────────────

        let credential = match parse_credential(credential_json_str) {
            Ok(c) => c,
            Err(msg) => {
                call_completion(completion, completion_ctx, Err(SshError::Other(msg)));
                return;
            }
        };

        // ── Connect (blocking on the calling thread) ───────────────────

        let request = SshConnectionRequest {
            host: host.to_string(),
            port,
            username: username.to_string(),
            credential,
            proxy_command: proxy_command.map(String::from),
        };

        let result = handle
            .inner
            .runtime
            .block_on(RusshSshClient::connect(request));

        match result {
            Ok(client) => {
                *lock(&handle.inner.client) = Some(client);
                call_completion(completion, completion_ctx, Ok(()));
            }
            Err(e) => {
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

/// Open a PTY shell on the connected SSH session.
///
/// After the shell is opened, the background read loop is automatically
/// started (if a read callback has already been installed by
/// [`bt_ssh_client_set_read_callback`]).
///
/// The completion callback is invoked on the *calling* thread.
///
/// # Parameters
///
/// - `handle`: opaque handle, must be connected.
/// - `cols`: terminal width in character cells.
/// - `rows`: terminal height in character cells.
/// - `width_px`: pixel width of the viewport (0 if unknown).
/// - `height_px`: pixel height of the viewport (0 if unknown).
/// - `completion`: callback invoked with the result.
/// - `completion_ctx`: opaque context for `completion`.
///
/// # Safety
///
/// See the module-level safety documentation.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bt_ssh_client_open_shell(
    handle: *mut SshClientHandle,
    cols: u16,
    rows: u16,
    width_px: u16,
    height_px: u16,
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

        let dims = PtyDimensions::with_pixels(cols, rows, width_px, height_px);
        let mut guard = lock(&handle.inner.client);
        let client = match guard.as_mut() {
            Some(c) => c,
            None => {
                call_completion(
                    completion,
                    completion_ctx,
                    Err(SshError::Other("not connected".into())),
                );
                return;
            }
        };

        let result = handle.inner.runtime.block_on(client.open_shell(dims));

        match result {
            Ok(()) => {
                // Release the client lock before potentially spawning the
                // read loop (which will need to lock the client).
                drop(guard);
                try_start_read_loop(handle);
                call_completion(completion, completion_ctx, Ok(()));
            }
            Err(e) => {
                call_completion(completion, completion_ctx, Err(e));
            }
        }
    });

    if result.is_err() {
        call_completion(
            completion,
            completion_ctx,
            Err(SshError::Other("Rust panic during open_shell".into())),
        );
    }
}

/// Install a read callback on the SSH client.
///
/// The callback is invoked for every chunk of data received from the SSH
/// channel.  If the shell is already open and no read thread is running,
/// calling this function automatically spawns the background read loop.
///
/// Use [`bt_ssh_client_clear_read_callback`] to unset an existing callback
/// (subsequent data will be silently dropped).
///
/// # Parameters
///
/// - `handle`: opaque handle.
/// - `sink`: callback function pointer.  Must be non-null.
/// - `sink_ctx`: opaque context threaded verbatim to `sink`.
///
/// # Safety
///
/// `sink` must be a valid function pointer valid for the lifetime of the
/// handle or until a subsequent call to [`bt_ssh_client_clear_read_callback`]
/// or [`bt_ssh_client_close`].  `sink_ctx` must remain valid for the same
/// duration.
#[no_mangle]
pub unsafe extern "C" fn bt_ssh_client_set_read_callback(
    handle: *mut SshClientHandle,
    sink: BtSSHOutputSink,
    sink_ctx: *mut c_void,
) {
    let _ = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }
        let handle = &*handle;

        let mut cb = lock(&handle.inner.read_callback);
        *cb = Some((sink, SinkContext(sink_ctx)));
        drop(cb);

        // If the shell is already open, try to start the read loop now.
        try_start_read_loop(handle);
    });
}

/// Clear a previously-installed read callback.
///
/// Subsequent data received from the SSH channel will be silently dropped
/// until a new callback is installed.
///
/// # Parameters
///
/// - `handle`: opaque handle.
///
/// # Safety
///
/// See the module-level safety documentation.  NULL handles are a no-op.
#[no_mangle]
pub unsafe extern "C" fn bt_ssh_client_clear_read_callback(handle: *mut SshClientHandle) {
    let _ = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }
        let handle = &*handle;
        let mut cb = lock(&handle.inner.read_callback);
        *cb = None;
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
/// `bytes` must be valid for `len` bytes.  Passing NULL with len > 0 is
/// undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn bt_ssh_client_write(
    handle: *mut SshClientHandle,
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
                    Err(SshError::Other("shell not opened".into())),
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
            Err(SshError::Other("Rust panic during write".into())),
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
/// - `width_px`: pixel width of the viewport (0 if unknown).
/// - `height_px`: pixel height of the viewport (0 if unknown).
/// - `completion`: callback invoked with the result.
/// - `completion_ctx`: opaque context for `completion`.
///
/// # Safety
///
/// See the module-level safety documentation.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bt_ssh_client_resize(
    handle: *mut SshClientHandle,
    cols: u16,
    rows: u16,
    width_px: u16,
    height_px: u16,
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

        let dims = PtyDimensions::with_pixels(cols, rows, width_px, height_px);
        let mut guard = lock(&handle.inner.client);
        let client = match guard.as_mut() {
            Some(c) => c,
            None => {
                call_completion(
                    completion,
                    completion_ctx,
                    Err(SshError::Other("shell not opened".into())),
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

/// Close the SSH connection and free the handle.
///
/// Signals the background read loop to stop, joins the read-loop thread,
/// calls [`SshClient::close`] on the connected client, and deallocates
/// the handle.
///
/// After this call the handle pointer is invalid and must not be used
/// again.  Safe to call with NULL.
///
/// # Safety
///
/// `handle` must be non-NULL and returned by [`bt_ssh_client_create`]
/// that has not already been passed to `bt_ssh_client_close`.
#[no_mangle]
pub unsafe extern "C" fn bt_ssh_client_close(handle: *mut SshClientHandle) {
    let _ = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }

        // Take ownership of the box — it drops when we're done.
        let handle = unsafe { Box::from_raw(handle) };

        // 1. Signal the read loop to stop.
        handle.inner.stop_read_loop.store(true, Ordering::SeqCst);

        // 2. Join the background read thread (if any).  The thread checks
        //    the stop flag at most READ_POLL_MS after signalling, so this
        //    join should resolve promptly.
        if let Some(thread) = lock(&handle.read_thread).take() {
            let _ = thread.join();
        }

        // 3. Take the client out of the Mutex (scoped so the MutexGuard
        //    drops before `handle` is deallocated).
        let client = { lock(&handle.inner.client).take() };
        if let Some(client) = client {
            let _ = handle.inner.runtime.block_on(client.close());
        }

        // 4. `handle` (the Box) is dropped here, which drops the
        //    `Arc<SshClientInner>`, and `SshClientInner` drops the
        //    tokio runtime last.
    });
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Spawn the background read-loop thread if (and only if) a read callback
/// is installed, the client is present, and no read thread is already
/// running.
///
/// Safe to call multiple times — subsequent calls are no-ops.
fn try_start_read_loop(handle: &SshClientHandle) {
    let has_callback = lock(&handle.inner.read_callback).is_some();
    let has_client = lock(&handle.inner.client).is_some();
    let already_running = lock(&handle.read_thread).is_some();

    if has_callback && has_client && !already_running {
        let inner = Arc::clone(&handle.inner);
        let builder_result = std::thread::Builder::new()
            .name("bt-ssh-read-loop".into())
            .spawn(move || {
                read_loop(inner);
            });
        match builder_result {
            Ok(thread) => {
                *lock(&handle.read_thread) = Some(thread);
            }
            Err(_) => {
                // Failed to spawn the read-loop OS thread.  The caller
                // will not receive data callbacks; the Swift side can
                // detect this condition by timing out.
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Error description FFI
// ---------------------------------------------------------------------------

thread_local! {
    /// Thread-local buffer for `bt_ssh_error_describe` — stores the formatted
    /// `CString` across the FFI boundary so the returned pointer stays valid
    /// until the next call from the same thread.
    static SSH_ERROR_DESCRIBE_BUF: RefCell<Option<CString>> = RefCell::new(None);
}

/// Produce a user-facing description for a [`BtSSHResultCode`], optionally
/// embedding a detail string.
///
/// The returned `*const c_char` points into a thread-local buffer that is
/// valid until the next call from the same thread.  Swift must copy the
/// string before calling this function again.
///
/// Pass `detail = NULL` for error variants that do not carry a detail.
///
/// The descriptions match the existing `TerminalSession.describe()` output.
///
/// # Safety
///
/// `detail` must be NULL or a valid nul-terminated UTF-8 C string borrowed
/// for the duration of the call.
#[no_mangle]
pub extern "C" fn bt_ssh_error_describe(
    code: BtSSHResultCode,
    detail: *const c_char,
) -> *const c_char {
    let detail_str = cstr(detail);
    let description = match (code, detail_str) {
        (BtSSHResultCode::BtSSHResultDnsResolution, _) => {
            "Cannot resolve host.".into()
        }
        (BtSSHResultCode::BtSSHResultTcpRefused, _) => {
            "Connection refused \u{2014} check host and port.".into()
        }
        (BtSSHResultCode::BtSSHResultTimeout, _) => {
            "Connection timed out.".into()
        }
        (BtSSHResultCode::BtSSHResultHandshakeFailed, Some(d)) => {
            format!("SSH handshake failed: {d}")
        }
        (BtSSHResultCode::BtSSHResultHandshakeFailed, None) => {
            "SSH handshake failed.".into()
        }
        (BtSSHResultCode::BtSSHResultAuthenticationFailed, _) => {
            "Authentication failed.".into()
        }
        (BtSSHResultCode::BtSSHResultPrivateKeyParse, _) => {
            "Cannot parse private key.".into()
        }
        (BtSSHResultCode::BtSSHResultPrivateKeyPassphraseRequired, _) => {
            "Private key requires a passphrase.".into()
        }
        (BtSSHResultCode::BtSSHResultHostKeyMismatch, Some(d)) => {
            format!("Host key changed.\n{d}")
        }
        (BtSSHResultCode::BtSSHResultHostKeyMismatch, None) => {
            "Host key changed.".into()
        }
        (BtSSHResultCode::BtSSHResultDisconnected, Some(d)) => {
            format!("Disconnected: {d}")
        }
        (BtSSHResultCode::BtSSHResultDisconnected, None) => {
            "Disconnected.".into()
        }
        (BtSSHResultCode::BtSSHResultPeerReset, _) => {
            "Connection reset by the remote host (network change or idle timeout). Tap to reconnect.".into()
        }
        (BtSSHResultCode::BtSSHResultShellExited, Some(d)) => {
            format!("Shell exited ({d}).")
        }
        (BtSSHResultCode::BtSSHResultShellExited, None) => {
            "Shell exited.".into()
        }
        (BtSSHResultCode::BtSSHResultOk, _) | (BtSSHResultCode::BtSSHResultOther, _) => {
            detail_str.unwrap_or("Unknown error.").to_string()
        }
    };

    let cstring = match CString::new(description) {
        Ok(s) => s,
        Err(_) => {
            // Fallback: null byte in the description (shouldn't happen).
            return b"Internal error.\0" as *const _ as *const c_char;
        }
    };
    let ptr = cstring.as_ptr();
    SSH_ERROR_DESCRIBE_BUF.with(|buf| {
        *buf.borrow_mut() = Some(cstring);
    });
    ptr
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Credential parsing ──────────────────────────────────────────────

    #[test]
    fn parse_password_credential() {
        let json = r#"{"type": "password", "password": "s3cret"}"#;
        let cred = parse_credential(json).unwrap();
        match cred {
            HostCredential::Password { password } => assert_eq!(password, "s3cret"),
            _ => panic!("expected Password"),
        }
    }

    #[test]
    fn parse_private_key_credential() {
        let json = r#"{"type": "private_key", "private_key": "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQo=", "passphrase": null}"#;
        let cred = parse_credential(json).unwrap();
        match cred {
            HostCredential::PrivateKey {
                private_key,
                passphrase,
            } => {
                assert!(private_key.starts_with(b"LS0tLS1CRUdJTi"));
                assert!(passphrase.is_none());
            }
            _ => panic!("expected PrivateKey"),
        }
    }

    #[test]
    fn parse_private_key_with_passphrase() {
        let json = r#"{"type": "private_key", "private_key": "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQo=", "passphrase": "hunter2"}"#;
        let cred = parse_credential(json).unwrap();
        match cred {
            HostCredential::PrivateKey {
                private_key: _,
                passphrase,
            } => {
                assert_eq!(passphrase, Some("hunter2".into()));
            }
            _ => panic!("expected PrivateKey"),
        }
    }

    #[test]
    fn parse_agent_credential() {
        let json = r#"{"type": "agent"}"#;
        let cred = parse_credential(json).unwrap();
        assert!(matches!(cred, HostCredential::Agent));
    }

    #[test]
    fn parse_invalid_credential() {
        assert!(parse_credential("not json").is_err());
        assert!(parse_credential(r#"{"type": "unknown"}"#).is_err());
        assert!(parse_credential(r#"{"type": "password"}"#).is_err()); // missing password field
    }

    // ── Result code mapping ─────────────────────────────────────────────

    #[test]
    fn result_code_mapping() {
        assert_eq!(
            result_code(&SshError::DnsResolution),
            BtSSHResultCode::BtSSHResultDnsResolution
        );
        assert_eq!(
            result_code(&SshError::TcpRefused),
            BtSSHResultCode::BtSSHResultTcpRefused
        );
        assert_eq!(
            result_code(&SshError::Timeout),
            BtSSHResultCode::BtSSHResultTimeout
        );
        assert_eq!(
            result_code(&SshError::HandshakeFailed("bad version".into())),
            BtSSHResultCode::BtSSHResultHandshakeFailed
        );
        assert_eq!(
            result_code(&SshError::AuthenticationFailed),
            BtSSHResultCode::BtSSHResultAuthenticationFailed
        );
        assert_eq!(
            result_code(&SshError::PrivateKeyParse),
            BtSSHResultCode::BtSSHResultPrivateKeyParse
        );
        assert_eq!(
            result_code(&SshError::PrivateKeyPassphraseRequired),
            BtSSHResultCode::BtSSHResultPrivateKeyPassphraseRequired
        );
        assert_eq!(
            result_code(&SshError::HostKeyMismatch {
                stored: "a".into(),
                remote: "b".into()
            }),
            BtSSHResultCode::BtSSHResultHostKeyMismatch
        );
        assert_eq!(
            result_code(&SshError::Disconnected("bye".into())),
            BtSSHResultCode::BtSSHResultDisconnected
        );
        assert_eq!(
            result_code(&SshError::PeerReset),
            BtSSHResultCode::BtSSHResultPeerReset
        );
        assert_eq!(
            result_code(&SshError::ShellExited(1)),
            BtSSHResultCode::BtSSHResultShellExited
        );
        assert_eq!(
            result_code(&SshError::Other("oops".into())),
            BtSSHResultCode::BtSSHResultOther
        );
        assert_eq!(result_code(&SshError::DnsResolution) as i32, 1);
    }

    // ── SshClientHandle layout ──────────────────────────────────────────

    /// Verify that `SshClientHandle` is `Send` (required for `Arc` + thread
    /// spawning).
    #[test]
    fn handle_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SshClientHandle>();
    }

    /// Verify that `SshClientInner` is `Send`.
    #[test]
    fn inner_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SshClientInner>();
    }

    // ── Lock helper ─────────────────────────────────────────────────────

    #[test]
    fn lock_recovers_from_poison() {
        let m: Mutex<i32> = Mutex::new(42);
        // Poison the mutex.
        let _ = std::panic::catch_unwind(|| {
            let _guard = m.lock().unwrap();
            panic!("deliberate panic to poison mutex");
        });
        // lock() should recover.
        let val = *lock(&m);
        assert_eq!(val, 42);
    }
}
