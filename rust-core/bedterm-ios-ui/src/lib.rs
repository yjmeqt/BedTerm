//! iOS UI layer for BedTerm.
//!
//! This crate is iOS-only — the entire compilation unit is gated on
//! `target_os = "ios"`. On every other target the crate compiles to an empty
//! library so the workspace still builds in CI / host tooling.
//!
//! Surface: a `UIViewController` subclass that hosts two text-input surfaces
//! (a Metal-backed `BtIosMetalInputView` and a UITextView composer), a
//! persistent keybar, and a DEBUG-only HUD. Focus is routed through
//! `BtIosKeyboardCoordinator`.

#![cfg(target_os = "ios")]

mod action_chip;
mod color;
mod coordinator;
mod debug_hud;
mod dpad;
mod geometry;
mod input_bar;
mod input_mode;
mod keybar;
mod metal_view;
mod text_input;
mod vc;

/// Opaque callback type fired when the in-VC back button is tapped.
pub type BtIosBackCallback = unsafe extern "C" fn(ctx: *mut std::ffi::c_void);

/// Create the iOS terminal `UIViewController *` (returned as `*mut c_void` so
/// the C header can stay type-agnostic).
///
/// - `on_back`: C callback fired when the back button is tapped (may be null).
/// - `ctx`: context pointer passed through to `on_back` (may be null).
///
/// The returned pointer is a **+1 retained** `UIViewController` that the
/// caller owns. Release via `bt_ios_release_vc`.
///
/// # Safety
/// `on_back` and `ctx` are stored and invoked on the main thread only.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_vc(
    on_back: Option<BtIosBackCallback>,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    vc::create_vc(on_back, ctx)
}

/// Release a `UIViewController *` previously returned by `bt_ios_create_vc`.
/// Safe to call with null.
///
/// # Safety
/// `vc_ptr` must be a pointer returned by `bt_ios_create_vc` and not yet
/// released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_release_vc(vc_ptr: *mut std::ffi::c_void) {
    vc::release_vc(vc_ptr);
}
