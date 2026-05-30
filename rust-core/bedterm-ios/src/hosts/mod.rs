//! Rust port of the Hosts screen (W24b).
//!
//! [`model`] is re-exported from `bedterm_app`.
//! [`hosts_vc`] (iOS-only) is `BtIosHostsListViewController`.

// Re-export pure model from bedterm-app
pub use bedterm_app::hosts::model;

pub mod hosts_vc;

use std::ffi::c_void;

/// C callback fired when the user taps the navigation-bar `+` button.
pub type BtIosHostsAddCallback = unsafe extern "C" fn(ctx: *mut c_void);
