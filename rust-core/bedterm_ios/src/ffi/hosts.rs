//! FFI surface — Rust Hosts list VC lifecycle.
//!
//! Mirrors the shape of `ffi::settings`: construct + release a Rust
//! `UIViewController *`. The single user-driven callback is `on_add`,
//! fired when the user taps the navigation-bar `+` button. Row taps and
//! swipes call back into Swift directly through `bt_swift_hosts_*` —
//! there's no per-row callback because Swift owns the connect / delete
//! orchestration end-to-end.

#![cfg(target_os = "ios")]

use crate::hosts::hosts_vc::{create_hosts_list_vc, release_hosts_list_vc};
use crate::hosts::BtIosHostsAddCallback;
use std::ffi::c_void;

/// Create the Rust-built Hosts list `UIViewController *` (returned as
/// opaque `*mut c_void`). +1 retained — release via
/// [`bt_ios_release_hosts_list_vc`].
///
/// - `on_add`: callback fired on the main thread when the user taps the
///   navigation-bar `+` button. The Swift host responds by presenting
///   the (still-SwiftUI) connect form sheet. May be NULL.
/// - `ctx`: opaque pointer threaded through to `on_add`. May be NULL.
///
/// # Safety
/// `on_add` is invoked on the main thread. `ctx` is never dereffed by
/// Rust; the Swift host owns its lifetime until the VC is released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_create_hosts_list_vc(
    on_add: Option<BtIosHostsAddCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    create_hosts_list_vc(on_add, ctx)
}

/// Release a Hosts list `UIViewController *` previously returned by
/// [`bt_ios_create_hosts_list_vc`]. Safe to call with NULL.
///
/// # Safety
/// `vc_ptr` must have been returned by `bt_ios_create_hosts_list_vc`
/// and not yet released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_release_hosts_list_vc(vc_ptr: *mut c_void) {
    release_hosts_list_vc(vc_ptr);
}
