//! Rust port of the SwiftUI ConnectionForm screen (W24c).
//!
//! [`model`] is pure (no UIKit gate) and host-testable. It defines
//! [`model::ConnectFormDraft`], [`model::AuthMode`] and the validation
//! rules that mirror Swift's `ConnectionFormViewModel.canSave` / `save`.
//!
//! [`connect_form_vm`] is the pure state machine that replaces the Swift
//! `ConnectionFormViewModel` — all field state, dirty tracking, validation
//! and secret-preservation logic runs here. The [`connect_form_vc`] calls
//! into it directly through the singleton [`crate::connect_form_vm::VM`].

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub mod model;

#[cfg(target_os = "ios")]
pub mod connect_form_vc;

#[cfg(target_os = "ios")]
use std::ffi::c_char;
#[cfg(target_os = "ios")]
use std::ffi::c_void;

/// Callback fired when the user successfully saves the form. `id_string`
/// is a UTF-8 nul-terminated C string of the saved entry's UUID; valid
/// only for the duration of the call. `connect_now` is true when the
/// caller wants to start a connect immediately (matches the
/// `connectOnSave` shortcut in the SwiftUI flow). Kept for internal
/// storage; FFI entry inlines the bare-fn type (see `ffi/vc.rs`).
#[cfg(target_os = "ios")]
pub type BtIosConnectFormDoneCallback =
    unsafe extern "C" fn(ctx: *mut c_void, id_string: *const c_char, connect_now: bool);

/// Callback fired when the user taps Cancel. Same rationale as
/// `BtIosConnectFormDoneCallback`.
#[cfg(target_os = "ios")]
pub type BtIosConnectFormCancelCallback = unsafe extern "C" fn(ctx: *mut c_void);
