//! FFI surface — `BtIosToasterView` factory.
//!
//! The `RootCoordinator` creates one toaster view and drives it through
//! the Rust API directly; only the factory export remains.

#![cfg(target_os = "ios")]

use crate::toaster;
use objc2::rc::Retained;
use std::os::raw::c_void;

/// Create a `BtIosToasterView *` (returned as `*mut c_void`). The result
/// is **+1 retained** and owned by the caller; release with
/// `Retained::from_raw`.
///
/// # Safety
/// Must be called on the main thread (UIKit construction).
#[no_mangle]
pub unsafe extern "C" fn bt_ios_toaster_view_new() -> *mut c_void {
    let view = toaster::create_toaster_view();
    Retained::into_raw(view) as *mut c_void
}
