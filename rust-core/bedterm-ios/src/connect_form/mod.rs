//! Rust port of the SwiftUI ConnectionForm screen (W24c).
//!
//! [`model`] is re-exported from `bedterm_app`.
//! The pure state machine lives in `bedterm_app::connect_form_vm`.

#![cfg_attr(not(target_os = "ios"), allow(unused_imports))]

// Re-export pure model from bedterm-app
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub use bedterm_app::connect_form::model;

#[cfg(target_os = "ios")]
pub mod connect_form_vc;

#[cfg(target_os = "ios")]
use std::ffi::c_char;
#[cfg(target_os = "ios")]
use std::ffi::c_void;

/// Callback fired when the user successfully saves the form.
#[cfg(target_os = "ios")]
pub type BtIosConnectFormDoneCallback =
    unsafe extern "C" fn(ctx: *mut c_void, id_string: *const c_char, connect_now: bool);

/// Callback fired when the user taps Cancel.
#[cfg(target_os = "ios")]
pub type BtIosConnectFormCancelCallback = unsafe extern "C" fn(ctx: *mut c_void);
