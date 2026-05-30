//! FFI surface — connect-form ViewModel callbacks.
//!
//! Only `bt_ios_connect_form_vm_set_callbacks` remains. The Swift
//! `@Observable` proxy + individual getter/setter exports were removed
//! in favour of the direct Rust VC driving the VM through the mutex.

#![cfg(target_os = "ios")]

use bedterm_app::connect_form_vm::{self, ConnectFormVM, SaveResultFn};
use std::ffi::{c_char, c_void};

fn vm_lock() -> std::sync::MutexGuard<'static, ConnectFormVM> {
    match connect_form_vm::VM.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Install the save-result side-effect callback.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_connect_form_vm_set_callbacks(
    save_result_cb: SaveResultFn,
    save_result_ctx: *mut c_void,
    _alert_cb: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
    _alert_ctx: *mut c_void,
) {
    vm_lock().set_callbacks(save_result_cb, save_result_ctx);
}
