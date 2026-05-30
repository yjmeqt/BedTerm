//! Shared C-ABI types for the SSH client / terminal session boundary.
//!
//! Only the types used by [`crate::terminal_session`] and
//! [`crate::ssh_client`] remain. The deprecated vtable bridge
//! (`BtSSHClientVTable`, `SSHBridgeHandle`, `bt_ios_register_ssh_bridge`)
//! was removed in favour of the direct `bt_terminal_session_*` API.

use std::ffi::c_void;

/// Result code matching the C header's `BtSSHResultCode` enum.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
pub enum BtSSHResultCode {
    BtSSHResultOk = 0,
    BtSSHResultDnsResolution = 1,
    BtSSHResultTcpRefused = 2,
    BtSSHResultTimeout = 3,
    BtSSHResultHandshakeFailed = 4,
    BtSSHResultAuthenticationFailed = 5,
    BtSSHResultHostKeyMismatch = 8,
    BtSSHResultDisconnected = 9,
    BtSSHResultPeerReset = 10,
    BtSSHResultShellExited = 11,
    BtSSHResultOther = 99,
}

/// Generic completion: `code` + optional detail `msg`.
pub type BtSSHCompletion =
    unsafe extern "C" fn(ctx: *mut c_void, code: BtSSHResultCode, msg: *const c_void, extra: i32);

/// Output-data sink. Called per inbound chunk on the main queue.
pub type BtSSHOutputSink = unsafe extern "C" fn(ctx: *mut c_void, bytes: *const u8, len: usize);
