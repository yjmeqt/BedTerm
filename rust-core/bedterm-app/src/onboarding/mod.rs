//! Onboarding state machine (pure, no UIKit).
//!
//! Defines [`OnboardingState`], [`HostKind`], [`Location`], [`Step`].

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub mod state;
