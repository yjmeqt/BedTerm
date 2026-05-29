//! Deprecated C-vtable SSH bridge.
//!
//! Rust now owns the SSH client directly (see [`crate::ffi::ssh_client_ffi`]).
//! The types `BtSSHResultCode`, `BtSSHCompletion`, and `BtSSHOutputSink`
//! are still used by the new FFI; the vtable bridge types
//! (`BtSSHClientVTable`, `SSHBridgeHandle`, `SSHBridge`) and entry points
//! (`bt_ios_register_ssh_bridge`, `bt_ios_ssh_bridge_release`) are kept
//! as harmless dead code for now.
//!
//! Full original design at `bedterm/docs/specs/swift-rust-ssh-bridge.md`.

use std::ffi::c_void;

/// Result code packed into completion callbacks. Mirrors `SSHError`
/// (see spec § "Error mapping"). Variants that carry detail strings
/// receive them via the `msg` slot (a retained `NSString *`, copied and
/// released by the Rust side).
// Variants are spelled with the `BtSSHResult*` prefix so the C header
// cbindgen generates matches what `SSHClientBridge.swift` already
// references (e.g. `BtSSHResultDnsResolution`). The type name keeps the
// `Code` suffix to match the Swift call sites' parameter type.
//
// `repr(C)` (rather than `repr(u32)`) so cbindgen emits the typedef-enum
// form `typedef enum BtSSHResultCode { … } BtSSHResultCode;` — which
// Swift imports as a named enum with each variant typed as `BtSSHResultCode`.
// The `repr(u32)` form emits a `typedef uint32_t BtSSHResultCode;` plus
// a separate `enum BtSSHResultCode { … }`, which Swift imports as two
// unrelated symbols (overload-resolution then fails on the call sites).
// ABI-wise the C `int`-sized enum matches the Swift `BtSSHResultCode`
// parameter that `SSHClientBridge` already passes.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    clippy::enum_variant_names
)]
pub enum BtSSHResultCode {
    BtSSHResultOk = 0,
    BtSSHResultDnsResolution = 1,
    BtSSHResultTcpRefused = 2,
    BtSSHResultTimeout = 3,
    BtSSHResultHandshakeFailed = 4,
    BtSSHResultAuthenticationFailed = 5,
    BtSSHResultPrivateKeyParse = 6,
    BtSSHResultPrivateKeyPassphraseRequired = 7,
    BtSSHResultHostKeyMismatch = 8,
    BtSSHResultDisconnected = 9,
    BtSSHResultPeerReset = 10,
    BtSSHResultShellExited = 11,
    BtSSHResultOther = 99,
}

/// C-compatible mirror of `SSHConnectionRequest`. Lifetimes: pointers
/// are valid only for the duration of the `connect` thunk call; Swift
/// copies anything it needs before returning.
#[repr(C)]
pub struct BtSSHConnectRequest {
    /// Pointer to a `HostCredential`-shaped opaque payload owned by
    /// Rust. The exact shape is TBD when `HostCredential` itself moves
    /// to Rust; for now Swift accepts an opaque `*const c_void` and
    /// resolves it via a side table.
    pub credential_opaque: *const c_void,
    pub cols: i32,
    pub rows: i32,
    /// UTF-8, nul-terminated. NULL == no bootstrap payload.
    pub bootstrap_payload: *const std::os::raw::c_char,
}

/// Generic completion: `code` + optional detail `msg` (retained
/// `NSString *`, Rust must release).
pub type BtSSHCompletion = unsafe extern "C" fn(
    ctx: *mut c_void,
    code: BtSSHResultCode,
    msg: *const c_void, // NSString * (retained) — may be null
    extra: i32,         // shell-exit code, etc.
);

/// Output-data sink installed by Rust. Called per inbound chunk on the
/// main queue. `bytes` is valid only for the duration of the call.
pub type BtSSHOutputSink = unsafe extern "C" fn(ctx: *mut c_void, bytes: *const u8, len: usize);

/// Function-pointer table filled by Swift's `SSHClientBridge` and
/// handed to Rust via `bt_ios_ssh_register`. See spec § "Swift-side
/// wrapper" for the contract of each thunk.
#[repr(C)]
pub struct BtSSHClientVTable {
    pub connect: unsafe extern "C" fn(
        ctx: *mut c_void,
        req: BtSSHConnectRequest,
        completion: BtSSHCompletion,
        completion_ctx: *mut c_void,
    ),
    pub write: unsafe extern "C" fn(
        ctx: *mut c_void,
        bytes: *const u8,
        len: usize,
        completion: BtSSHCompletion,
        completion_ctx: *mut c_void,
    ),
    pub resize: unsafe extern "C" fn(
        ctx: *mut c_void,
        cols: i32,
        rows: i32,
        completion: BtSSHCompletion,
        completion_ctx: *mut c_void,
    ),
    pub disconnect: unsafe extern "C" fn(
        ctx: *mut c_void,
        completion: BtSSHCompletion,
        completion_ctx: *mut c_void,
    ),
    pub set_output_sink:
        unsafe extern "C" fn(ctx: *mut c_void, sink: BtSSHOutputSink, sink_ctx: *mut c_void),
    /// Balance the +1 retain Swift gave us when constructing `ctx`.
    pub release: unsafe extern "C" fn(ctx: *mut c_void),
}

/// Rust-owned handle. Holds the vtable + ctx returned by
/// `bt_ssh_bridge_make` (Swift side). `Drop` calls `release(ctx)` to
/// balance Swift's retain.
pub struct SSHBridgeHandle {
    vtable: BtSSHClientVTable,
    ctx: *mut c_void,
}

impl SSHBridgeHandle {
    /// Wrap a freshly-registered bridge — `ctx` is a +1 retained
    /// `Unmanaged<SSHClientBridge>` opaque pointer.
    pub fn new(vtable: BtSSHClientVTable, ctx: *mut c_void) -> Self {
        Self { vtable, ctx }
    }

    /// Raw context pointer — exposed so the session can pin lifetimes
    /// when handing the handle off to the FFI layer.
    pub fn ctx(&self) -> *mut c_void {
        self.ctx
    }
}

impl Drop for SSHBridgeHandle {
    fn drop(&mut self) {
        // SAFETY: `ctx` was produced by Swift's `bt_ssh_bridge_make`
        // with a matching +1 retain. We balance it exactly once.
        unsafe { (self.vtable.release)(self.ctx) };
    }
}

/// Rust-facing trait that the (future) `TerminalSession` port codes
/// against. Implemented by `SSHBridgeHandle`; a Rust-side mock can also
/// implement it for unit tests without touching FFI.
pub trait SSHBridge {
    fn connect(&self, req: BtSSHConnectRequest, completion: BtSSHCompletion, ctx: *mut c_void);
    fn write(&self, bytes: &[u8], completion: BtSSHCompletion, ctx: *mut c_void);
    fn resize(&self, cols: i32, rows: i32, completion: BtSSHCompletion, ctx: *mut c_void);
    fn disconnect(&self, completion: BtSSHCompletion, ctx: *mut c_void);
    fn set_output_sink(&self, sink: BtSSHOutputSink, ctx: *mut c_void);
}

impl SSHBridge for SSHBridgeHandle {
    fn connect(&self, req: BtSSHConnectRequest, completion: BtSSHCompletion, ctx: *mut c_void) {
        unsafe { (self.vtable.connect)(self.ctx, req, completion, ctx) };
    }

    fn write(&self, bytes: &[u8], completion: BtSSHCompletion, ctx: *mut c_void) {
        unsafe { (self.vtable.write)(self.ctx, bytes.as_ptr(), bytes.len(), completion, ctx) };
    }

    fn resize(&self, cols: i32, rows: i32, completion: BtSSHCompletion, ctx: *mut c_void) {
        unsafe { (self.vtable.resize)(self.ctx, cols, rows, completion, ctx) };
    }

    fn disconnect(&self, completion: BtSSHCompletion, ctx: *mut c_void) {
        unsafe { (self.vtable.disconnect)(self.ctx, completion, ctx) };
    }

    fn set_output_sink(&self, sink: BtSSHOutputSink, ctx: *mut c_void) {
        unsafe { (self.vtable.set_output_sink)(self.ctx, sink, ctx) };
    }
}

/// Register a Swift-built SSH bridge. The `vtable` is copied by value;
/// `ctx` ownership transfers to the returned handle (the handle's `Drop`
/// invokes `vtable.release(ctx)`).
///
/// Returns a +1 owned `*mut SSHBridgeHandle`; release with
/// `bt_ios_ssh_bridge_release`.
///
/// # Safety
/// `vtable` must point to a fully-populated `BtSSHClientVTable`. `ctx`
/// must be a valid retained handle. Caller must not free `ctx` itself —
/// the returned handle owns it.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_register_ssh_bridge(
    vtable: *const BtSSHClientVTable,
    ctx: *mut c_void,
) -> *mut SSHBridgeHandle {
    if vtable.is_null() {
        return std::ptr::null_mut();
    }
    let vtable = std::ptr::read(vtable);
    Box::into_raw(Box::new(SSHBridgeHandle::new(vtable, ctx)))
}

/// Release an `SSHBridgeHandle *` previously returned by
/// `bt_ios_register_ssh_bridge`. Safe to call with null.
///
/// # Safety
/// `handle` must be a pointer returned by `bt_ios_register_ssh_bridge`
/// and not yet released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_ssh_bridge_release(handle: *mut SSHBridgeHandle) {
    if handle.is_null() {
        return;
    }
    let _ = Box::from_raw(handle);
}
