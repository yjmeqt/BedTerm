//! A Rust-backed UIViewController exposed via C FFI.
//!
//! This is an experimental crate. The VC hosts two text-input surfaces:
//! a Metal-backed `BtRsMetalInputView` (view1) and a UITextView composer
//! (view2). Focus is routed through `BtRsKeyboardCoordinator`.

#[cfg(target_os = "ios")]
mod geometry;
#[cfg(target_os = "ios")]
mod vc;

/// Opaque callback type.
pub type BtRsBackCallback = unsafe extern "C" fn(ctx: *mut std::ffi::c_void);

/// Create a `UIViewController *` (returned as `*mut c_void` so the C header
/// stays type-agnostic).
///
/// - `on_back`: C callback fired when the back button is tapped (may be null).
/// - `ctx`: context pointer passed through to `on_back` (may be null).
///
/// The returned pointer is a **+1 retained** `UIViewController` that the
/// caller owns. Release via `bt_rs_terminal_release_vc`.
///
/// # Safety
/// `on_back` and `ctx` are stored and invoked on the main thread only.
#[no_mangle]
pub unsafe extern "C" fn bt_rs_terminal_create_vc(
    on_back: Option<BtRsBackCallback>,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    #[cfg(target_os = "ios")]
    {
        vc::create_vc(on_back, ctx)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (on_back, ctx);
        std::ptr::null_mut()
    }
}

/// Release a `UIViewController *` previously returned by
/// `bt_rs_terminal_create_vc`. Safe to call with null.
///
/// # Safety
/// `vc_ptr` must be a pointer returned by `bt_rs_terminal_create_vc` and
/// not yet released.
#[no_mangle]
pub unsafe extern "C" fn bt_rs_terminal_release_vc(vc_ptr: *mut std::ffi::c_void) {
    #[cfg(target_os = "ios")]
    {
        vc::release_vc(vc_ptr);
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = vc_ptr;
    }
}
