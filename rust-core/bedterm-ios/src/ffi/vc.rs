//! FFI surface — view controller lifecycle.
//!
//! Construction + release of the iOS terminal `UIViewController` and
//! the host-key mismatch review VC.

use crate::host_key_mismatch_vc;
use crate::vc;

/// Create the iOS terminal `UIViewController *` (returned as `*mut c_void` so
/// the C header can stay type-agnostic).
///
/// - `on_back`: C callback fired when the back button is tapped (may be null).
/// - `ctx`: context pointer passed through to `on_back` (may be null).
///
/// The returned pointer is a **+1 retained** `UIViewController` that the
/// caller owns. Release via `bt_ios_release_vc`.
///
/// # Safety
/// `on_back` and `ctx` are stored and invoked on the main thread only.
pub(crate) unsafe fn bt_ios_create_vc(
    // Inline the bare-fn type rather than `Option<BtIosBackCallback>`
    // so cbindgen emits a plain nullable function pointer (it only
    // unwraps `Option<extern fn>` when the inner type is a bare fn,
    // not a `Type::Path` typedef).
    on_back: Option<unsafe extern "C" fn(ctx: *mut std::ffi::c_void)>,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    vc::create_vc(on_back, ctx)
}

pub(crate) unsafe fn bt_ios_release_vc(vc_ptr: *mut std::ffi::c_void) {
    vc::release_vc(vc_ptr);
}

pub(crate) unsafe fn bt_ios_create_mismatch_vc(
    on_trust: host_key_mismatch_vc::MismatchCallback,
    on_reject: host_key_mismatch_vc::MismatchCallback,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    unsafe { host_key_mismatch_vc::create_mismatch_vc(on_trust, on_reject, ctx) }
}

pub(crate) unsafe fn bt_ios_release_mismatch_vc(vc_ptr: *mut std::ffi::c_void) {
    unsafe { host_key_mismatch_vc::release_mismatch_vc(vc_ptr) };
}
