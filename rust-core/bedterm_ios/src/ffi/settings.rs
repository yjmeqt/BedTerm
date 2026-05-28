//! FFI surface — Rust Settings sheet lifecycle + persisted settings.
//!
//! * `bt_ios_create_settings_vc` / `bt_ios_release_settings_vc` —
//!   construct + release the modal Settings sheet `UIViewController *`.
//! * `bt_ios_settings_*` — read / write the persisted boolean settings
//!   that the sheet exposes. Backed by `NSUserDefaults.standard` via
//!   [`crate::settings_store`]. Swift consumers (hosts list, terminal
//!   bootstrap) call these directly; there is no Swift-side cache.

#![cfg(target_os = "ios")]

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
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_settings_vc(
    // Inline the bare-fn type so cbindgen emits a nullable C function
    // pointer (it doesn't unwrap `Option<TypeAlias>` — see ffi/vc.rs).
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
#[no_mangle]
pub unsafe extern "C" fn bt_ios_release_settings_vc(vc_ptr: *mut std::ffi::c_void) {
    settings_vc::release_settings_vc(vc_ptr);
}

// ── Persisted settings ──────────────────────────────────────────────────
//
// All four accessors are main-thread callable; `NSUserDefaults` itself is
// thread-safe but the app's UIKit consumers run on the main actor, so
// callers don't need to hop.

/// Read the "show command blocks (Warp-style)" setting. Defaults to
/// `false` (beta opt-in) when no value has ever been written.
#[no_mangle]
pub extern "C" fn bt_ios_settings_show_command_blocks() -> bool {
    settings_store::show_command_blocks()
}

/// Persist the "show command blocks" setting.
#[no_mangle]
pub extern "C" fn bt_ios_settings_set_show_command_blocks(value: bool) {
    settings_store::set_show_command_blocks(value);
}

/// True iff the user has finished the onboarding flow. Defaults to
/// `false` on first launch — same key the previous Swift
/// `OnboardingPersistenceBridge` wrote.
#[no_mangle]
pub extern "C" fn bt_ios_settings_onboarding_completed() -> bool {
    settings_store::onboarding_completed()
}

/// Persist the onboarding-completion flag. Called from
/// `onboarding::coordinator` once the user lands on the final step.
#[no_mangle]
pub extern "C" fn bt_ios_settings_set_onboarding_completed(value: bool) {
    settings_store::set_onboarding_completed(value);
}
