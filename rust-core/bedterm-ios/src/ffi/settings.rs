//! FFI surface — Rust Settings sheet lifecycle + persisted settings.
//!
//! * `bt_ios_create_settings_vc` / `bt_ios_release_settings_vc` —
//!   construct + release the modal Settings sheet `UIViewController *`.
//! * `bt_ios_settings_onboarding_completed` — read the persisted
//!   onboarding-completion flag (used by RootCoordinator + SceneDelegate).

use crate::settings_store;
use crate::settings_vc;

/// Create the Rust-built Settings `UIViewController *` (returned as
/// `*mut c_void`). +1 retained — release via `bt_ios_release_settings_vc`.
///
/// - `on_done`: callback fired on the main thread when the user taps
///   the navigation-bar Done button (may be NULL).
/// - `ctx`: opaque pointer threaded through to `on_done` (may be NULL).
///
/// # Safety
/// `on_done` is invoked on the main thread. `ctx` is never dereffed by
/// Rust; the Swift host owns its lifetime until either it clears the
/// pair or the VC is released.
pub unsafe extern "C" fn bt_ios_create_settings_vc(
    on_done: Option<unsafe extern "C" fn(ctx: *mut std::ffi::c_void)>,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    settings_vc::create_settings_vc(on_done, ctx)
}

/// Release a Settings `UIViewController *` previously returned by
/// `bt_ios_create_settings_vc`. Safe to call with NULL.
///
/// # Safety
/// `vc_ptr` must have been returned by `bt_ios_create_settings_vc` and
/// not yet released.
pub unsafe extern "C" fn bt_ios_release_settings_vc(vc_ptr: *mut std::ffi::c_void) {
    settings_vc::release_settings_vc(vc_ptr);
}

/// True iff the user has finished the onboarding flow. Defaults to
/// `false` on first launch.
#[no_mangle]
pub extern "C" fn bt_ios_settings_onboarding_completed() -> bool {
    settings_store::onboarding_completed()
}
