//! Rust port of the Hosts screen (W24b phase 1).
//!
//! [`model`] is pure (no UIKit, no iOS gate) and host-testable. It
//! defines [`HostListEntry`] plus the JSON marshalling helpers used by
//! the Swift→Rust bridge to push the saved-hosts snapshot across the FFI
//! seam in a single call.
//!
//! [`bridge`] declares the `extern "C"` Swift-side accessors the VC
//! reaches for (snapshot, delete, connect, add).
//!
//! [`hosts_vc`] (iOS-only) is `BtIosHostsListViewController`, a
//! `UIViewController` subclass that lays out one `list_row` per saved
//! host inside a `UIScrollView`. Rows dispatch tap → connect, swipe →
//! delete; the navbar `+` button fires the `on_add` host callback so
//! Swift can present the (still-SwiftUI) connect form sheet.
//!
//! Connect orchestration (toasters, swap dialog, mismatch dialog,
//! terminal push) stays in Swift's existing `HostsViewModel`; this VC
//! only signals which row the user tapped.

// `model` is pure; on non-iOS hosts the only consumers are the unit
// tests, so dead-code warnings are silenced at module scope there.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub mod model;

#[cfg(target_os = "ios")]
pub mod bridge;
#[cfg(target_os = "ios")]
pub mod hosts_vc;

#[cfg(target_os = "ios")]
use std::ffi::c_void;

/// C callback fired when the user taps the navigation-bar `+` button on
/// the Rust hosts list VC. Swift owns the response (presenting the
/// SwiftUI connect-form sheet).
#[cfg(target_os = "ios")]
pub type BtIosHostsAddCallback = unsafe extern "C" fn(ctx: *mut c_void);
