//! FFI surface — SSH host-key fingerprint store.
//!
//! Wraps [`crate::host_key_store`] so the Swift `HostKeyStore` shim can
//! route every per-host Keychain read/write through Rust. Caller-owned
//! strings are heap-allocated via `CString::into_raw` and must be freed
//! via [`bt_ios_host_keys_free_string`].

#![cfg(target_os = "ios")]

use crate::host_key_store::{self, Verdict};
use std::ffi::{c_char, CStr, CString};

fn cstr<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s)
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

/// Load the stored fingerprint for `host:port`. Returns NULL when none
/// is stored. Caller frees via [`bt_ios_host_keys_free_string`].
///
/// # Safety
/// `host` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_host_keys_load(host: *const c_char, port: u16) -> *mut c_char {
    let Some(host) = cstr(host) else {
        return std::ptr::null_mut();
    };
    match host_key_store::load(host, port) {
        Some(s) => into_c(s),
        None => std::ptr::null_mut(),
    }
}

/// Free a string returned by any `bt_ios_host_keys_*` accessor. NULL-safe.
///
/// # Safety
/// `ptr` must have been returned by a `bt_ios_host_keys_*` accessor and
/// not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_host_keys_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    drop(unsafe { CString::from_raw(ptr) });
}

/// Persist `fingerprint` for `host:port`. Returns `false` on Keychain
/// error or invalid input.
///
/// # Safety
/// `host` and `fingerprint` are UTF-8 nul-terminated C strings borrowed
/// for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_host_keys_save(
    host: *const c_char,
    port: u16,
    fingerprint: *const c_char,
) -> bool {
    let Some(host) = cstr(host) else { return false };
    let Some(fp) = cstr(fingerprint) else {
        return false;
    };
    host_key_store::save(host, port, fp)
}

/// Drop the stored fingerprint for `host:port`. No-op when none stored.
///
/// # Safety
/// `host` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_host_keys_delete(host: *const c_char, port: u16) {
    let Some(host) = cstr(host) else { return };
    host_key_store::delete(host, port);
}

/// Compare `remote` against the stored fingerprint for `host:port`.
/// Returns the verdict discriminant:
///   0 = Match, 1 = Mismatch, 2 = Unknown.
/// When the result is `Mismatch (1)` and `out_stored` is non-NULL,
/// `*out_stored` is set to a newly-allocated UTF-8 C string carrying
/// the stored fingerprint; caller frees via
/// [`bt_ios_host_keys_free_string`]. For other verdicts `*out_stored`
/// is set to NULL.
///
/// # Safety
/// `host` and `remote` are UTF-8 nul-terminated C strings borrowed for
/// the call. `out_stored` may be NULL.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_host_keys_verify(
    host: *const c_char,
    port: u16,
    remote: *const c_char,
    out_stored: *mut *mut c_char,
) -> i32 {
    if !out_stored.is_null() {
        unsafe { *out_stored = std::ptr::null_mut() };
    }
    let Some(host) = cstr(host) else { return 2 };
    let Some(remote) = cstr(remote) else { return 2 };
    match host_key_store::verify(host, port, remote) {
        Verdict::Match => 0,
        Verdict::Mismatch { stored } => {
            if !out_stored.is_null() {
                unsafe { *out_stored = into_c(stored) };
            }
            1
        }
        Verdict::Unknown => 2,
    }
}

/// Test-only seam — route subsequent reads/writes to a per-test
/// in-process backend so simulator-backed unit tests don't pollute the
/// production Keychain. Pass NULL to restore the production backend.
///
/// # Safety
/// `service`, when non-NULL, is a UTF-8 nul-terminated C string borrowed
/// for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_host_keys_set_test_service(service: *const c_char) {
    let svc = if service.is_null() {
        None
    } else {
        (unsafe { CStr::from_ptr(service) }).to_str().ok()
    };
    host_key_store::set_test_override(svc);
}
