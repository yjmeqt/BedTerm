//! Rust port of the SwiftUI ConnectionForm screen (W24c).
//!
//! [`model`] is re-exported from `bedterm_app`.
//! The pure state machine lives in `bedterm_app::connect_form_vm`.

// Re-export pure model from bedterm-app
pub use bedterm_app::connect_form::model;

pub mod connect_form_vc;

use std::ffi::c_char;
use std::ffi::c_void;

/// Callback fired when the user successfully saves the form.
pub type BtIosConnectFormDoneCallback =
    unsafe extern "C" fn(ctx: *mut c_void, id_string: *const c_char, connect_now: bool);

/// Callback fired when the user taps Cancel.
pub type BtIosConnectFormCancelCallback = unsafe extern "C" fn(ctx: *mut c_void);
