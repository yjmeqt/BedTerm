//! Rust onboarding flow (R10).
//!
//! Pure [`state`] is re-exported from `bedterm_app`.
//! iOS-specific VCs and coordinator live here.

#![cfg_attr(not(target_os = "ios"), allow(unused_imports))]

// Re-export pure state from bedterm-app
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub use bedterm_app::onboarding::state;

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

#[cfg(target_os = "ios")]
pub type BtIosOnboardingChoiceCallback = unsafe extern "C" fn(ctx: *mut c_void, choice: i32);
#[cfg(target_os = "ios")]
pub type BtIosOnboardingContinueCallback = unsafe extern "C" fn(ctx: *mut c_void);
#[cfg(target_os = "ios")]
pub type BtIosOnboardingFlowCompletedCallback = unsafe extern "C" fn(ctx: *mut c_void);
#[cfg(target_os = "ios")]
pub type BtIosRequestLocalNetworkCallback =
    unsafe extern "C" fn(ctx: *mut c_void, completion: unsafe extern "C" fn(*mut c_void));
