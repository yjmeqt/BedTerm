//! FFI surface — onboarding step view controllers.
//!
//! Construction for the four R10 onboarding-step VCs ported from SwiftUI.
//! Each `bt_ios_create_onboarding_*_vc` returns a +1 retained
//! `UIViewController *` (opaque `void *`) that the caller owns; release
//! via `bt_ios_release_vc` (the existing single-VC release path is
//! generic over `UIViewController *`).

#![cfg(target_os = "ios")]

use crate::onboarding::coordinator::create_flow_vc;
use crate::onboarding::host_kind_vc::create_host_kind_vc;
use crate::onboarding::local_permission_vc::create_local_permission_vc;
use crate::onboarding::location_vc::create_location_vc;
use crate::onboarding::mac_tutorial_vc::create_mac_tutorial_vc;
use std::ffi::c_void;

/// Create the host-kind step VC (`onboarding/host_kind_vc.rs`).
/// `on_choice(ctx, choice)`: `choice == 0` → macOS, `choice == 1` → Linux/other.
///
/// # Safety
/// `on_choice` and `ctx` are stored and invoked on the main thread only.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_onboarding_host_kind_vc(
    // Inline the bare-fn type so cbindgen emits a nullable C function
    // pointer (it doesn't unwrap `Option<TypeAlias>` — see ffi/vc.rs).
    on_choice: Option<unsafe extern "C" fn(ctx: *mut c_void, choice: i32)>,
    ctx: *mut c_void,
) -> *mut c_void {
    create_host_kind_vc(on_choice, ctx)
}

/// Create the location step VC.
/// `on_choice(ctx, choice)`: `0` → same-Wi-Fi, `1` → remote.
///
/// # Safety
/// Same as `bt_ios_create_onboarding_host_kind_vc`.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_onboarding_location_vc(
    // Inline the bare-fn type so cbindgen emits a nullable C function
    // pointer (it doesn't unwrap `Option<TypeAlias>` — see ffi/vc.rs).
    on_choice: Option<unsafe extern "C" fn(ctx: *mut c_void, choice: i32)>,
    ctx: *mut c_void,
) -> *mut c_void {
    create_location_vc(on_choice, ctx)
}

/// Create the macOS tutorial step VC. Fires `on_continue(ctx)` once when
/// the user taps Continue.
///
/// # Safety
/// Same as the other onboarding-VC entry points.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_onboarding_mac_tutorial_vc(
    on_continue: Option<unsafe extern "C" fn(ctx: *mut c_void)>,
    ctx: *mut c_void,
) -> *mut c_void {
    create_mac_tutorial_vc(on_continue, ctx)
}

/// Create the local-network-permission terminator step VC. `is_remote`
/// selects the "All set" copy variant (remote host); pass `false` for the
/// same-Wi-Fi prose. Fires `on_continue(ctx)` once on tap; the Swift
/// coordinator is responsible for the `LocalNetworkPrewarmer` Bonjour
/// probe (the VC has no async story).
///
/// # Safety
/// Same as the other onboarding-VC entry points.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_onboarding_local_permission_vc(
    on_continue: Option<unsafe extern "C" fn(ctx: *mut c_void)>,
    ctx: *mut c_void,
    is_remote: bool,
) -> *mut c_void {
    create_local_permission_vc(on_continue, ctx, is_remote)
}

/// Create the full onboarding-flow VC — a `UINavigationController`
/// subclass (`BtIosOnboardingFlowVC`) that owns the
/// `OnboardingState` state machine and pushes each step VC as the user
/// advances. The Swift host installs the returned VC as its window root
/// and reacts to `on_completed(ctx)` by swapping to the hosts root.
///
/// The returned pointer is +1 retained; release with `bt_ios_release_vc`.
///
/// # Safety
/// `on_completed` and `ctx` are stored and invoked on the main thread only.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_onboarding_flow_vc(
    on_completed: Option<unsafe extern "C" fn(ctx: *mut c_void)>,
    ctx: *mut c_void,
) -> *mut c_void {
    create_flow_vc(on_completed, ctx)
}
