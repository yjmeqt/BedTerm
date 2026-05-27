//! Rust port of the SwiftUI ConnectionForm screen (W24c).
//!
//! [`model`] is pure (no UIKit gate) and host-testable. It defines
//! [`model::ConnectFormDraft`], [`model::AuthMode`] and the validation
//! rules that mirror Swift's `ConnectionFormViewModel.canSave` / `save`.
//!
//! [`bridge`] declares the `extern "C"` Swift-side accessors
//! ([`bridge::bt_swift_connect_form_prefill_json`],
//! [`bridge::bt_swift_connect_form_save`], …). Swift owns persistence —
//! Rust ships a validated draft over the FFI and Swift's
//! `ConnectionFormViewModel.save` does the keychain + ordering writes.
//!
//! [`connect_form_vc`] (iOS-only) is `BtIosConnectFormViewController`,
//! a `UIViewController` subclass containing a scroll view + vertical
//! stack of form sections (Identity / Connection / Authentication).
//! Cancel + Save bar buttons fire the FFI callbacks; the Save path
//! validates locally before hitting the Swift bridge.
//!
//! TODO(localization): English copy is hardcoded here; Swift owns the
//! localized variants. Once the flag flips, the strings will be routed
//! through a Swift-side provider.

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub mod model;

#[cfg(target_os = "ios")]
pub mod bridge;
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
/// `connectOnSave` shortcut in the SwiftUI flow).
#[cfg(target_os = "ios")]
pub type BtIosConnectFormDoneCallback =
    unsafe extern "C" fn(ctx: *mut c_void, id_string: *const c_char, connect_now: bool);

/// Callback fired when the user taps Cancel.
#[cfg(target_os = "ios")]
pub type BtIosConnectFormCancelCallback = unsafe extern "C" fn(ctx: *mut c_void);
