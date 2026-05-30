//! FFI surface — onboarding flow view controller.
//!
//! Only the full-flow VC (`bt_ios_create_onboarding_flow_vc`) remains.
//! The individual step VCs are created internally by the coordinator;
//! their FFI exports were removed.

use crate::onboarding::coordinator::create_flow_vc;
use std::ffi::c_void;

/// Create the full onboarding-flow VC — a `UINavigationController`
/// subclass (`BtIosOnboardingFlowVC`) that owns the `OnboardingState`
/// state machine and pushes each step VC as the user advances.
///
/// The returned pointer is +1 retained; release with `bt_ios_release_vc`.
///
/// - `on_completed`: fired once when onboarding finishes.
/// - `ctx`: opaque host context threaded into `on_completed`.
/// - `request_local_network`: injected by Swift to trigger the Bonjour-based
///   Local Network permission prompt. When NULL the permission step is
///   skipped (harmless: the OS prompts on first actual LAN connection).
///   Called with `(ctx, completion)` — Swift runs the probe and calls
///   `completion(ctx)` on the main thread when the OS resolves the prompt.
///
/// # Safety
/// All callbacks are stored and invoked on the main thread only.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_onboarding_flow_vc(
    on_completed: Option<unsafe extern "C" fn(ctx: *mut c_void)>,
    ctx: *mut c_void,
    request_local_network: Option<
        unsafe extern "C" fn(ctx: *mut c_void, completion: unsafe extern "C" fn(*mut c_void)),
    >,
) -> *mut c_void {
    create_flow_vc(on_completed, ctx, request_local_network)
}
