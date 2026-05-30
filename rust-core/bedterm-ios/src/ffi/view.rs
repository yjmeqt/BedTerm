//! FFI surface — `BtIosMetalInputView` accessors.
//!
//! Resolve and feed the Metal-backed input view embedded in the VC.
//! Bytes flow in via `bt_ios_view_feed_bytes`; the send and resize
//! callbacks were removed — the VC owns those paths directly in Rust.

#![cfg(target_os = "ios")]

use crate::metal_view;

/// Resolve the `BtIosMetalInputView *` embedded inside a VC returned by
/// `bt_ios_create_vc`. Returns NULL when the view didn't construct
/// (headless tests).
///
/// # Safety
/// Main thread. `vc_ptr` must be a live VC pointer.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_vc_metal_view(
    vc_ptr: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    if vc_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let vc_obj = vc_ptr as *mut objc2::runtime::AnyObject;
    let mv: *const metal_view::BtIosMetalInputView = objc2::msg_send![&*vc_obj, btIosMetalView];
    mv as *mut std::ffi::c_void
}

/// Feed raw terminal bytes into the `BtIosMetalInputView`'s owned grid.
/// `view_ptr` must be a +1 retained `BtIosMetalInputView *` (e.g. obtained
/// from the host VC). The call is a no-op when any argument is null / empty.
///
/// # Safety
/// Must be called on the main thread. `view_ptr` must point to a live
/// `BtIosMetalInputView`. `bytes` must be valid for `len` bytes for the
/// duration of the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_view_feed_bytes(
    view_ptr: *mut std::ffi::c_void,
    bytes: *const u8,
    len: usize,
) {
    if view_ptr.is_null() || bytes.is_null() || len == 0 {
        return;
    }
    let view = &*(view_ptr as *const metal_view::BtIosMetalInputView);
    let slice = std::slice::from_raw_parts(bytes, len);
    view.feed_bytes(slice);
}
