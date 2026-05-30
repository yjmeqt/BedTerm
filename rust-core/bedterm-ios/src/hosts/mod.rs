//! Rust port of the Hosts screen (W24b).
//!
//! [`model`] is pure (no UIKit, no iOS gate) and host-testable. It
//! defines [`HostListEntry`] plus the JSON marshalling helpers used
//! to push the saved-hosts snapshot across the FFI seam.
//!
//! [`hosts_vc`] (iOS-only) is `BtIosHostsListViewController`, a
//! `UIViewController` subclass that lays out one `list_row` per saved
//! host inside a `UIScrollView`. Rows dispatch tap → connect (through
//! the Rust hosts VM, `bt_ios_hosts_vm_request_connect`), swipe →
//! delete; the navbar `+` button fires the `on_add` host callback to
//! the Rust coordinator which pushes the Rust connect-form VC.
//!
//! Connect orchestration (toasters, swap dialog, mismatch dialog,
//! terminal push) is all in Rust (`RootCoordinator`).

// `model` is pure; on non-iOS hosts the only consumers are the unit
// tests, so dead-code warnings are silenced at module scope there.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub mod model;

#[cfg(target_os = "ios")]
pub mod hosts_vc;

#[cfg(target_os = "ios")]
use std::ffi::c_void;

/// C callback fired when the user taps the navigation-bar `+` button on
/// the Rust hosts list VC. Swift owns the response (presenting the
/// SwiftUI connect-form sheet). Kept for internal storage; the FFI
/// entry inlines the bare-fn type for cbindgen-friendly emission (see
/// `ffi/vc.rs`).
#[cfg(target_os = "ios")]
pub type BtIosHostsAddCallback = unsafe extern "C" fn(ctx: *mut c_void);
