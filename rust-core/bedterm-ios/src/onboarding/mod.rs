//! Rust onboarding flow (R10).
//!
//! Pure [`state`] is re-exported from `bedterm_app`.
//! iOS-specific VCs and coordinator live here.

// Re-export pure state from bedterm-app
pub use bedterm_app::onboarding::state;

pub(crate) mod choice_button;
pub(crate) mod coordinator;
pub(crate) mod host_kind_vc;
pub(crate) mod local_permission_vc;
pub(crate) mod location_vc;
pub(crate) mod mac_tutorial_vc;

use std::ffi::c_void;

pub type BtIosOnboardingChoiceCallback = unsafe extern "C" fn(ctx: *mut c_void, choice: i32);
pub type BtIosOnboardingContinueCallback = unsafe extern "C" fn(ctx: *mut c_void);
pub type BtIosOnboardingFlowCompletedCallback = unsafe extern "C" fn(ctx: *mut c_void);
pub type BtIosRequestLocalNetworkCallback =
    unsafe extern "C" fn(ctx: *mut c_void, completion: unsafe extern "C" fn(*mut c_void));
