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
use crate::hosts_store;
use crate::hosts_vm::{self, Action};
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
    // Inline the bare-fn type so cbindgen emits a nullable C function
    // pointer (it doesn't unwrap `Option<TypeAlias>` — see ffi/vc.rs).
    on_add: Option<unsafe extern "C" fn(ctx: *mut c_void)>,
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

// ── Hosts ViewModel ─────────────────────────────────────────────────────
//
// Singleton state machine owned by [`crate::hosts_vm`]. Each FFI call
// locks the mutex briefly, mutates, then returns. Swift mirrors the
// resulting state into its @Observable properties on every mutator
// so `withObservationTracking` in `HostsConnectController` keeps
// firing on the same key paths.
//
// Compound state (PendingMismatch / SwapConfirmation / DeleteConfirmation)
// is returned as JSON to keep the FFI surface narrow; the structs match
// the Swift mirror via #[serde(rename = ...)] on the Rust side.
// All `*mut c_char` returns must be freed via `bt_ios_hosts_free_string`.

fn vm_lock() -> std::sync::MutexGuard<'static, hosts_vm::HostsVM> {
    // Poison just means a thread panicked while holding the lock; we
    // recover the state and keep going.
    match hosts_vm::VM.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

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

fn optional_string(opt: Option<String>) -> *mut c_char {
    match opt {
        Some(s) => into_c(s),
        None => std::ptr::null_mut(),
    }
}

fn optional_json<T: serde::Serialize>(opt: Option<&T>) -> *mut c_char {
    match opt {
        Some(v) => match serde_json::to_string(v) {
            Ok(s) => into_c(s),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

/// Action discriminant for `bt_ios_hosts_vm_request_connect`,
/// `bt_ios_hosts_vm_retry_after_mismatch`, and `…_end_live_session`:
/// 0 = None, 1 = Connect (payload UUID in `*out_uuid`), 2 = Disconnect.
fn encode_action(action: Action, out_uuid: *mut *mut c_char) -> i32 {
    if !out_uuid.is_null() {
        unsafe { *out_uuid = std::ptr::null_mut() };
    }
    match action {
        Action::None => 0,
        Action::Connect(id) => {
            if !out_uuid.is_null() {
                unsafe { *out_uuid = into_c(id) };
            }
            1
        }
        Action::Disconnect => 2,
    }
}

/// Reload the entries array from the saved-hosts store. Reads the
/// display-shape JSON via [`crate::hosts_store::list_snapshot_json`]
/// and parses it into the VM.
#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_load_from_store() {
    let json = hosts_store::list_snapshot_json();
    vm_lock().set_entries_from_snapshot(&json);
}

/// UI-test seam — append stub entries (HostsStoreInjection.current)
/// that aren't backed by Keychain. JSON must be an array matching the
/// display snapshot schema (id, label, host, port, username, authIsKey).
///
/// # Safety
/// `json` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_merge_injected(json: *const c_char) {
    let Some(j) = cstr(json) else { return };
    vm_lock().merge_injected(j);
}

/// Mark the VM as having failed its most recent load — Swift uses
/// this when `UIApplication.shared.isProtectedDataAvailable` returns
/// false.
#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_set_load_failed(value: bool) {
    vm_lock().set_load_failed(value);
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_load_failed() -> bool {
    vm_lock().load_failed
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_entries_json() -> *mut c_char {
    let vm = vm_lock();
    match serde_json::to_string(&vm.entries) {
        Ok(s) => into_c(s),
        Err(_) => into_c(String::from("[]")),
    }
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_in_flight_id() -> *mut c_char {
    optional_string(vm_lock().in_flight_id.clone())
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_current_session_id() -> *mut c_char {
    optional_string(vm_lock().current_session_id.clone())
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_pending_mismatch_json() -> *mut c_char {
    optional_json(vm_lock().pending_mismatch.as_ref())
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_swap_confirmation_json() -> *mut c_char {
    optional_json(vm_lock().swap_confirmation.as_ref())
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_delete_confirmation_json() -> *mut c_char {
    optional_json(vm_lock().delete_confirmation.as_ref())
}

/// Returns the display name for `uuid` (label, or "user@host" fallback,
/// or "" when no match). Caller frees.
///
/// # Safety
/// `uuid` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_display_name_for(uuid: *const c_char) -> *mut c_char {
    let id = cstr(uuid).unwrap_or("");
    into_c(vm_lock().display_name_for(id))
}

/// Row's Connect tap. Drives the swap-confirm dance. Returns the action
/// discriminant (see `encode_action`); when 1 (Connect), `*out_uuid` is
/// the UUID Swift should kick the SSH attempt for.
///
/// # Safety
/// `uuid` is a UTF-8 nul-terminated C string. `out_uuid` may be NULL;
/// when non-NULL, the callee writes either NULL or an owned C string
/// that the caller must free via `bt_ios_hosts_free_string`.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_request_connect(
    uuid: *const c_char,
    out_uuid: *mut *mut c_char,
) -> i32 {
    let Some(id) = cstr(uuid) else {
        return encode_action(Action::None, out_uuid);
    };
    encode_action(vm_lock().request_connect(id), out_uuid)
}

/// Accept the swap dialog. Returns the target UUID via `*out_uuid` when
/// a swap was pending, else NULL. The Swift caller is responsible for
/// disconnecting its `lastSession` *before* kicking off the new attempt.
///
/// # Safety
/// `out_uuid` may be NULL.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_confirm_swap(out_uuid: *mut *mut c_char) {
    let target = vm_lock().confirm_swap();
    if !out_uuid.is_null() {
        unsafe { *out_uuid = optional_string(target) };
    }
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_cancel_swap() {
    vm_lock().cancel_swap();
}

/// Swift's SSH attempt produced a session. No-op when the in-flight id
/// doesn't match (late callback from a cancelled task).
///
/// # Safety
/// `uuid` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_connect_completed_session(uuid: *const c_char) {
    let Some(id) = cstr(uuid) else { return };
    vm_lock().connect_completed_session(id);
}

/// Swift's SSH attempt hit a host-key mismatch.
///
/// # Safety
/// All `*const c_char` args are UTF-8 nul-terminated C strings borrowed
/// for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_connect_completed_mismatch(
    uuid: *const c_char,
    stored: *const c_char,
    remote: *const c_char,
    host: *const c_char,
    port: u16,
) {
    let Some(id) = cstr(uuid) else { return };
    let stored = cstr(stored).unwrap_or("");
    let remote = cstr(remote).unwrap_or("");
    let host = cstr(host).unwrap_or("");
    vm_lock().connect_completed_mismatch(id, stored, remote, host, port);
}

/// Swift's SSH attempt failed (network, auth, etc.). Swift fires its own
/// `onConnectError` toast; the VM just clears in-flight.
///
/// # Safety
/// `uuid` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_connect_completed_error(uuid: *const c_char) {
    let Some(id) = cstr(uuid) else { return };
    vm_lock().connect_completed_error(id);
}

/// Retry after the user trusts a new host key. Returns Connect / None
/// via the action encoding.
///
/// # Safety
/// `out_uuid` may be NULL.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_retry_after_mismatch(out_uuid: *mut *mut c_char) -> i32 {
    let target = vm_lock().retry_after_mismatch();
    let action = match target {
        Some(id) => Action::Connect(id),
        None => Action::None,
    };
    encode_action(action, out_uuid)
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_clear_mismatch() {
    vm_lock().clear_mismatch();
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_session_ended() {
    vm_lock().session_ended();
}

/// End the live session. Returns Disconnect / None via the action
/// encoding; no payload (Swift already owns the session ref).
#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_end_live_session() -> i32 {
    encode_action(vm_lock().end_live_session(), std::ptr::null_mut())
}

/// Surface the delete-confirmation dialog state for `uuid`. Swift then
/// reads `bt_ios_hosts_vm_delete_confirmation_json` to drive the alert.
///
/// # Safety
/// `uuid` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_request_delete(uuid: *const c_char) {
    let Some(id) = cstr(uuid) else { return };
    vm_lock().request_delete(id);
}

/// Commit the pending delete. Returns the target UUID as an owned
/// string (caller frees), or NULL when no delete was pending. Drops
/// the entry from the mirrored array. Swift must follow up with
/// `bt_ios_hosts_delete` to remove the Keychain blob.
#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_confirm_delete() -> *mut c_char {
    optional_string(vm_lock().confirm_delete())
}

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_cancel_delete() {
    vm_lock().cancel_delete();
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
