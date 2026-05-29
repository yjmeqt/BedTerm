//! FFI surface — connect-form ViewModel singleton.
//!
//! Mirrors the per-namespace pattern in [`crate::ffi::hosts`]. Each call
//! briefly locks the [`crate::connect_form_vm::VM`] mutex, mutates or
//! reads, then returns. The Swift `ConnectionFormViewModel` is a thin
//! `@Observable` proxy that mirrors state into its key paths on every
//! mutator so existing `withObservationTracking` observers keep firing.
//!
//! Persistence (writing the resolved `SavedHost` blob to the Keychain)
//! stays in Swift because `HostCredential` is Codable on the Swift side.
//! `bt_ios_connect_form_vm_try_save` returns a JSON `SaveOutcome` that
//! Swift consumes to materialise + write a `SavedHost`.

#![cfg(target_os = "ios")]

use crate::connect_form::model::{normalized_host, normalized_port};
use crate::connect_form_vm::{self, ConnectFormVM};
use std::ffi::{c_char, CStr, CString};

fn vm_lock() -> std::sync::MutexGuard<'static, ConnectFormVM> {
    match connect_form_vm::VM.lock() {
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

/// Free a string returned by any `bt_ios_connect_form_vm_*` getter that
/// returns a `*mut c_char`. NULL-safe.
///
/// # Safety
/// `ptr` must have been returned by one of the `bt_ios_connect_form_vm_*`
/// FFI exports and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    drop(unsafe { CString::from_raw(ptr) });
}

/// Reset the VM to an empty Add-mode draft (port "22").
#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_reset_to_add() {
    vm_lock().reset_to_add();
}

/// Prefill the VM for an Edit-mode session. Existing secrets are passed
/// as `*const c_char` (NULL → unset). `existing_private_key` is a raw
/// byte buffer with `private_key_len` bytes; pass `(NULL, 0)` for no
/// key. All inputs are borrowed for the duration of the call.
///
/// # Safety
/// All `*const c_char` pointers must be NULL or valid UTF-8
/// nul-terminated strings borrowed for the call. `existing_private_key`
/// may be NULL iff `private_key_len == 0`.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bt_ios_connect_form_vm_prefill_edit(
    id: *const c_char,
    label: *const c_char,
    host: *const c_char,
    port: u16,
    username: *const c_char,
    auth_is_key: bool,
    existing_password: *const c_char,
    existing_private_key: *const u8,
    private_key_len: usize,
    existing_passphrase: *const c_char,
) {
    let id_s = cstr(id).unwrap_or("").to_string();
    let label_s = cstr(label).unwrap_or("").to_string();
    let host_s = cstr(host).unwrap_or("").to_string();
    let user_s = cstr(username).unwrap_or("").to_string();
    let pw = cstr(existing_password).map(|s| s.to_string());
    let pass = cstr(existing_passphrase).map(|s| s.to_string());
    let pk = if existing_private_key.is_null() || private_key_len == 0 {
        None
    } else {
        Some(unsafe { std::slice::from_raw_parts(existing_private_key, private_key_len) }.to_vec())
    };
    vm_lock().prefill_edit(
        id_s,
        label_s,
        host_s,
        port,
        user_s,
        auth_is_key,
        pw,
        pk,
        pass,
    );
}

// ─── Setters ───────────────────────────────────────────────────────

/// # Safety
/// `value` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_set_label(value: *const c_char) {
    let v = cstr(value).unwrap_or("").to_string();
    vm_lock().set_label(v);
}

/// # Safety
/// `value` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_set_host(value: *const c_char) {
    let v = cstr(value).unwrap_or("").to_string();
    vm_lock().set_host(v);
}

/// # Safety
/// `value` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_set_port_text(value: *const c_char) {
    let v = cstr(value).unwrap_or("").to_string();
    vm_lock().set_port_text(v);
}

/// # Safety
/// `value` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_set_username(value: *const c_char) {
    let v = cstr(value).unwrap_or("").to_string();
    vm_lock().set_username(v);
}

/// # Safety
/// `value` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_set_password(value: *const c_char) {
    let v = cstr(value).unwrap_or("").to_string();
    vm_lock().set_password(v);
}

/// # Safety
/// `value` is a UTF-8 nul-terminated C string borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_set_passphrase(value: *const c_char) {
    let v = cstr(value).unwrap_or("").to_string();
    vm_lock().set_passphrase(v);
}

/// # Safety
/// `bytes` may be NULL iff `len == 0`.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_set_private_key_bytes(
    bytes: *const u8,
    len: usize,
) {
    let v: Vec<u8> = if bytes.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(bytes, len) }.to_vec()
    };
    vm_lock().set_private_key_bytes(v);
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_set_using_key(value: bool) {
    vm_lock().set_using_key(value);
}

// ─── Getters ───────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_label() -> *mut c_char {
    into_c(vm_lock().label.clone())
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_host() -> *mut c_char {
    into_c(vm_lock().host.clone())
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_port_text() -> *mut c_char {
    into_c(vm_lock().port.clone())
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_username() -> *mut c_char {
    into_c(vm_lock().username.clone())
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_is_using_key() -> bool {
    vm_lock().is_using_key
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_has_password() -> bool {
    vm_lock().has_password()
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_has_private_key() -> bool {
    vm_lock().has_private_key()
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_has_passphrase() -> bool {
    vm_lock().has_passphrase()
}

#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_can_save() -> bool {
    vm_lock().can_save()
}

/// Returns the current error message (set by the most recent
/// `try_save` failure), or NULL when none. Caller frees via
/// `bt_ios_connect_form_vm_free_string`.
#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_error_message() -> *mut c_char {
    optional_string(vm_lock().error_message.clone())
}

/// Validate without mutating. Returns NULL when persistable, else a
/// localized error message owned by the caller (free via
/// `bt_ios_connect_form_vm_free_string`).
#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_validate() -> *mut c_char {
    optional_string(vm_lock().validate())
}

/// Attempt to save. Returns the JSON-encoded [`SaveOutcome`] (caller
/// frees) on success, or NULL on validation failure (call
/// `bt_ios_connect_form_vm_error_message` to retrieve the message).
#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_try_save() -> *mut c_char {
    let mut vm = vm_lock();
    match vm.try_save() {
        Ok(outcome) => match serde_json::to_string(&outcome) {
            Ok(json) => into_c(json),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Returns the normalized host from the current VM host field (splits
/// `host:port` paste, strips brackets around IPv6 addresses, trims
/// whitespace). Caller frees via
/// [`bt_ios_connect_form_vm_free_string`].
#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_normalized_host() -> *mut c_char {
    let host = vm_lock().host.clone();
    into_c(normalized_host(&host))
}

/// Returns the normalized port from the current VM host + port fields.
/// Returns 0 when no valid port is recoverable (port 0 is never valid
/// for SSH, so 0 serves as a safe sentinel for "no port").
#[no_mangle]
pub extern "C" fn bt_ios_connect_form_vm_normalized_port() -> u16 {
    let vm = vm_lock();
    normalized_port(&vm.host, &vm.port).unwrap_or(0)
}
