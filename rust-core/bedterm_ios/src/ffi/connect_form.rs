//! FFI surface — Rust connect-form VC lifecycle (W24c).
//!
//! Construct + release a `BtIosConnectFormViewController *`. The caller
//! wraps the returned VC in a `UINavigationController` (or pushes onto an
//! existing nav stack) and observes the user's choice via `on_done` /
//! `on_cancel`. Persistence still runs through Swift's
//! `ConnectionFormViewModel.save`; this VC only collects + validates input.

#![cfg(target_os = "ios")]

use crate::connect_form::connect_form_vc::{create_connect_form_vc, release_connect_form_vc};
use crate::connect_form::{BtIosConnectFormCancelCallback, BtIosConnectFormDoneCallback};
use std::ffi::{c_char, c_void, CStr};

/// Create a connect-form VC.
///
/// - `editing_id_or_null`: UTF-8, nul-terminated UUID string of an
///   existing host to edit, or NULL for "Add Host".
/// - `connect_on_save`: when true, the Save bar button reads
///   "Save & Connect" — mirrors the SwiftUI
///   `ConnectionFormScreen.primaryActionTitle` branch.
/// - `on_done`: fires on the main thread with the saved entry's UUID
///   string once the user successfully saves. The string is borrowed
///   for the duration of the call.
/// - `on_cancel`: fires when the user taps Cancel.
/// - `ctx`: opaque pointer threaded into both callbacks.
///
/// Returns +1 retained; release via [`bt_ios_release_connect_form_vc`].
///
/// # Safety
/// Callbacks invoke on the main thread. `ctx` is never dereffed by Rust.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_connect_form_vc(
    editing_id_or_null: *const c_char,
    connect_on_save: bool,
    on_done: Option<BtIosConnectFormDoneCallback>,
    on_cancel: Option<BtIosConnectFormCancelCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    let id_string = if editing_id_or_null.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(editing_id_or_null) }
                .to_string_lossy()
                .into_owned(),
        )
    };
    create_connect_form_vc(id_string, connect_on_save, on_done, on_cancel, ctx)
}

/// Release a connect-form `UIViewController *` previously returned by
/// [`bt_ios_create_connect_form_vc`]. Safe to call with NULL.
///
/// # Safety
/// `vc_ptr` must have been returned by `bt_ios_create_connect_form_vc`
/// and not yet released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_release_connect_form_vc(vc_ptr: *mut c_void) {
    release_connect_form_vc(vc_ptr);
}
