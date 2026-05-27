//! FFI surface — Rust Hosts list VC lifecycle + saved-hosts persistence.
//!
//! VC lifecycle mirrors `ffi::settings`: construct + release a Rust
//! `UIViewController *`. The single user-driven callback is `on_add`,
//! fired when the user taps the navigation-bar `+` button. Row taps and
//! swipes call back into Swift directly through `bt_swift_hosts_*` —
//! there's no per-row callback because Swift owns the connect / delete
//! orchestration end-to-end.
//!
//! The persistence half (`bt_ios_hosts_*`) wraps [`crate::hosts_store`] so
//! the Swift `HostsStore` shim can route every Keychain + UserDefaults
//! read/write through Rust. Swift retains the Codable `SavedHost` shape;
//! Rust sees opaque blobs except when building the display-snapshot JSON.

#![cfg(target_os = "ios")]

use crate::hosts::hosts_vc::{create_hosts_list_vc, release_hosts_list_vc};
use crate::hosts::BtIosHostsAddCallback;
use crate::hosts_store;
use std::ffi::{c_char, c_void, CStr, CString};

/// Create the Rust-built Hosts list `UIViewController *` (returned as
/// opaque `*mut c_void`). +1 retained — release via
/// [`bt_ios_release_hosts_list_vc`].
///
/// - `on_add`: callback fired on the main thread when the user taps the
///   navigation-bar `+` button. The Swift host responds by presenting
///   the (still-SwiftUI) connect form sheet. May be NULL.
/// - `ctx`: opaque pointer threaded through to `on_add`. May be NULL.
///
/// # Safety
/// `on_add` is invoked on the main thread. `ctx` is never dereffed by
/// Rust; the Swift host owns its lifetime until the VC is released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_hosts_list_vc(
    on_add: Option<BtIosHostsAddCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    create_hosts_list_vc(on_add, ctx)
}

/// Release a Hosts list `UIViewController *` previously returned by
/// [`bt_ios_create_hosts_list_vc`]. Safe to call with NULL.
///
/// # Safety
/// `vc_ptr` must have been returned by `bt_ios_create_hosts_list_vc`
/// and not yet released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_release_hosts_list_vc(vc_ptr: *mut c_void) {
    release_hosts_list_vc(vc_ptr);
}

// ── Hosts store ──────────────────────────────────────────────────────────

/// Return the saved-hosts display snapshot as a `+1` retained UTF-8
/// C string. Free via `bt_ios_hosts_free_string`. Never NULL — an empty
/// store yields `"[]"`.
#[no_mangle]
pub extern "C" fn bt_ios_hosts_snapshot_json() -> *mut c_char {
    let json = hosts_store::list_snapshot_json();
    CString::new(json)
        .map(CString::into_raw)
        .unwrap_or_else(|_| CString::new("[]").expect("static literal").into_raw())
}

/// Free a string returned by `bt_ios_hosts_snapshot_json`. NULL-safe.
///
/// # Safety
/// `ptr` must have been returned by `bt_ios_hosts_snapshot_json` and not
/// yet freed.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    drop(unsafe { CString::from_raw(ptr) });
}

/// Load the raw `SavedHost` JSON blob for `uuid`. Returns NULL when no
/// item is stored. `*out_len` is set to the buffer length on success.
/// Free with `bt_ios_hosts_free_blob`.
///
/// # Safety
/// `uuid` is a UTF-8 nul-terminated C string borrowed for the call.
/// `out_len` may be NULL.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_load_blob(
    uuid: *const c_char,
    out_len: *mut usize,
) -> *mut u8 {
    if uuid.is_null() {
        if !out_len.is_null() {
            unsafe { *out_len = 0 };
        }
        return std::ptr::null_mut();
    }
    let Ok(uuid_str) = (unsafe { CStr::from_ptr(uuid) }).to_str() else {
        if !out_len.is_null() {
            unsafe { *out_len = 0 };
        }
        return std::ptr::null_mut();
    };
    match hosts_store::load_blob(uuid_str) {
        Some(bytes) => {
            let len = bytes.len();
            let boxed = bytes.into_boxed_slice();
            let ptr = Box::into_raw(boxed) as *mut u8;
            if !out_len.is_null() {
                unsafe { *out_len = len };
            }
            ptr
        }
        None => {
            if !out_len.is_null() {
                unsafe { *out_len = 0 };
            }
            std::ptr::null_mut()
        }
    }
}

/// Free a blob returned by `bt_ios_hosts_load_blob`. NULL-safe.
///
/// # Safety
/// `(ptr, len)` must have been returned together by
/// `bt_ios_hosts_load_blob` and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_free_blob(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
    drop(unsafe { Box::from_raw(slice as *mut [u8]) });
}

/// Persist `bytes` as the `SavedHost` blob for `uuid`. Returns `false`
/// on Keychain error or invalid input.
///
/// # Safety
/// `uuid` is a UTF-8 nul-terminated C string borrowed for the call.
/// `bytes` may be NULL only if `len == 0`.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_save_blob(
    uuid: *const c_char,
    bytes: *const u8,
    len: usize,
) -> bool {
    if uuid.is_null() {
        return false;
    }
    let Ok(uuid_str) = (unsafe { CStr::from_ptr(uuid) }).to_str() else {
        return false;
    };
    let slice: &[u8] = if len == 0 || bytes.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    hosts_store::save(uuid_str, slice)
}

/// Delete the entry for `uuid` from the Keychain + order index. No-op
/// when `uuid` is missing.
///
/// # Safety
/// `uuid` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_delete(uuid: *const c_char) {
    if uuid.is_null() {
        return;
    }
    let Ok(uuid_str) = (unsafe { CStr::from_ptr(uuid) }).to_str() else {
        return;
    };
    hosts_store::delete(uuid_str);
}

/// Test-only seam — wipe just the order index, leaving Keychain blobs
/// intact. Used by the reconciliation test to assert that the next
/// `bt_ios_hosts_snapshot_json` call rebuilds the order from surviving
/// items.
#[no_mangle]
pub extern "C" fn bt_ios_hosts_test_clear_order() {
    hosts_store::test_clear_order();
}

/// Test-only seam — route subsequent reads/writes to a per-test
/// `(service, order_key)` pair so simulator-backed unit tests don't
/// pollute the production Keychain / UserDefaults entries. Pass
/// `(NULL, NULL)` to restore the production defaults.
///
/// # Safety
/// Both pointers, when non-NULL, must be UTF-8 nul-terminated C strings
/// borrowed for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_set_test_service(
    service: *const c_char,
    order_key: *const c_char,
) {
    let svc = if service.is_null() {
        None
    } else {
        (unsafe { CStr::from_ptr(service) }).to_str().ok()
    };
    let ord = if order_key.is_null() {
        None
    } else {
        (unsafe { CStr::from_ptr(order_key) }).to_str().ok()
    };
    hosts_store::set_test_override(svc, ord);
}
