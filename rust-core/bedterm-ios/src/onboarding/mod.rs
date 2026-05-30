//! Rust onboarding flow (R10).
//!
//! After the W23d wave, Rust owns the full state machine + step transitions
//! for the four R10 onboarding steps. Swift retains only `LocalNetworkPrewarmer`
//! (Network.framework wrapper) and the `onboardingCompleted` UserDefaults
//! flag, both reached through thin `@_cdecl` bridges.
//!
//! - [`state`] is pure (no iOS gating) and host-testable.
//! - [`coordinator`] owns the active `UINavigationController`, the
//!   [`state::OnboardingState`] instance, and pushes each step's VC on
//!   the user's choice.
//! - [`host_kind_vc`], [`location_vc`], [`mac_tutorial_vc`],
//!   [`local_permission_vc`] are the four step VCs (UIKit).
//!
//! Step int conventions (kept for legacy per-VC FFI used by tests):
//! - HostKindVC: `0 = .macOS`, `1 = .other`
//! - LocationVC: `0 = .sameWifi`, `1 = .remote`
//! - MacTutorialVC + LocalPermissionVC: continue-only (single callback).

// `state` is pure (no UIKit, no iOS gate) so it can be unit-tested on a
// macOS host. On non-iOS builds the items aren't consumed — silence the
// dead-code lint there rather than at every item.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub mod state;

#[cfg(target_os = "ios")]
pub(crate) mod choice_button;
#[cfg(target_os = "ios")]
pub(crate) mod coordinator;
#[cfg(target_os = "ios")]
pub(crate) mod host_kind_vc;
#[cfg(target_os = "ios")]
pub(crate) mod local_permission_vc;
#[cfg(target_os = "ios")]
pub(crate) mod location_vc;
#[cfg(target_os = "ios")]
pub(crate) mod mac_tutorial_vc;

#[cfg(target_os = "ios")]
use std::ffi::c_void;

/// C callback fired when the user picks one of the picker-step choices.
/// `choice` is the discriminant (see module-level doc for the mapping).
/// Kept for internal storage; FFI entry inlines the bare-fn type (see
/// `ffi/vc.rs`).
#[cfg(target_os = "ios")]
pub type BtIosOnboardingChoiceCallback = unsafe extern "C" fn(ctx: *mut c_void, choice: i32);

/// C callback fired when the user taps the primary "Continue" button on
/// the MacTutorial or LocalPermission step.
#[cfg(target_os = "ios")]
pub type BtIosOnboardingContinueCallback = unsafe extern "C" fn(ctx: *mut c_void);

/// C callback fired once when the entire onboarding flow has completed.
/// The Swift host should swap to the hosts root in response.
#[cfg(target_os = "ios")]
pub type BtIosOnboardingFlowCompletedCallback = unsafe extern "C" fn(ctx: *mut c_void);

/// C callback that Rust calls to trigger the iOS "Local Network" permission
/// prompt (Bonjour probe). Injected by the Swift host at flow-VC creation
/// time so Rust never reaches for a global `@_cdecl` symbol.
///
/// - `ctx`: the flow-VC pointer (`*const BtIosOnboardingFlowVC`).
/// - `completion`: Rust-side callback to invoke once the OS resolves the
///   permission prompt (fired on the main thread).
#[cfg(target_os = "ios")]
pub type BtIosRequestLocalNetworkCallback =
    unsafe extern "C" fn(ctx: *mut c_void, completion: unsafe extern "C" fn(*mut c_void));
