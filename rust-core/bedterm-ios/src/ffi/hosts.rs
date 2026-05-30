//! FFI surface — Rust Hosts list VC lifecycle + saved-hosts persistence.
//!
//! VC lifecycle: construct + release a Rust `UIViewController *`. The
//! `on_add` callback fires on `+` button tap. Row taps route through
//! `bt_ios_hosts_vm_request_connect` → VM callback → RootCoordinator.
//!
//! The persistence half wraps [`crate::hosts_store`] for Keychain blob
//! reads/writes.

#![cfg(target_os = "ios")]

use crate::hosts::hosts_vc::{create_hosts_list_vc, release_hosts_list_vc};
use crate::hosts_store;
use crate::hosts_vm::{self, Action, AlertFn, CallbackCtx, ConnectFn, DisconnectFn};
use std::ffi::{c_char, c_void, CStr, CString};

// ── VC lifecycle ──────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_hosts_list_vc(
    on_add: Option<unsafe extern "C" fn(ctx: *mut c_void)>,
    ctx: *mut c_void,
) -> *mut c_void {
    create_hosts_list_vc(on_add, ctx)
}

#[no_mangle]
pub unsafe extern "C" fn bt_ios_release_hosts_list_vc(vc_ptr: *mut c_void) {
    release_hosts_list_vc(vc_ptr);
}

// ── Persistence ───────────────────────────────────────────────────────────

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

// ── Hosts ViewModel ───────────────────────────────────────────────────────

fn vm_lock() -> std::sync::MutexGuard<'static, hosts_vm::HostsVM> {
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

#[no_mangle]
pub extern "C" fn bt_ios_hosts_vm_load_from_store() {
    let json = hosts_store::list_snapshot_json();
    vm_lock().set_entries_from_snapshot(&json);
}

#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_set_callbacks(
    connect_cb: ConnectFn,
    connect_ctx: *mut c_void,
    disconnect_cb: DisconnectFn,
    disconnect_ctx: *mut c_void,
    alert_cb: AlertFn,
    alert_ctx: *mut c_void,
) {
    vm_lock().set_callbacks(
        connect_cb,
        CallbackCtx(connect_ctx),
        disconnect_cb,
        CallbackCtx(disconnect_ctx),
        alert_cb,
        CallbackCtx(alert_ctx),
    );
}

#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_request_connect(
    uuid: *const c_char,
    out_uuid: *mut *mut c_char,
) -> i32 {
    let Some(id) = cstr(uuid) else {
        return encode_action(Action::None, out_uuid);
    };
    let mut vm = vm_lock();
    let action = vm.request_connect(id);
    let connect = vm.connect_cb;
    let connect_ctx = vm.connect_ctx;
    drop(vm);
    if let Action::Connect(ref cid) = action {
        if let Some(cb) = connect {
            let c_str = CString::new(cid.as_str()).unwrap_or_default();
            unsafe { cb(connect_ctx.0, c_str.as_ptr()) };
        }
    }
    encode_action(action, out_uuid)
}

#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_connect_completed_session(uuid: *const c_char) {
    let Some(id) = cstr(uuid) else { return };
    vm_lock().connect_completed_session(id);
}

#[no_mangle]
pub unsafe extern "C" fn bt_ios_hosts_vm_connect_completed_error(uuid: *const c_char) {
    let Some(id) = cstr(uuid) else { return };
    vm_lock().connect_completed_error(id);
}
