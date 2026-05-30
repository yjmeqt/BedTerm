//! Rust port of the Hosts screen (W24b).
//!
//! [`model`] is re-exported from `bedterm_app`.
//! [`hosts_vc`] (iOS-only) is `BtIosHostsListViewController`.

#![cfg_attr(not(target_os = "ios"), allow(unused_imports))]

// Re-export pure model from bedterm-app
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub use bedterm_app::hosts::model;

#[cfg(target_os = "ios")]
pub mod hosts_vc;

#[cfg(target_os = "ios")]
use std::ffi::c_void;

/// C callback fired when the user taps the navigation-bar `+` button.
#[cfg(target_os = "ios")]
pub type BtIosHostsAddCallback = unsafe extern "C" fn(ctx: *mut c_void);
