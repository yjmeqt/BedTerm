//! Rust->Swift bridge declarations for the W24b Hosts list VC.
//!
//! The Swift `HostsBridge.swift` `@_cdecl` shims implement these symbols.
//!
//! The snapshot and delete primitives were previously carried by
//! `bt_swift_hosts_snapshot_json` / `bt_swift_hosts_free_snapshot` /
//! `bt_swift_hosts_delete` — these are now called directly from
//! `crate::hosts_store` to cut the Rust->Swift->Rust round trip.
//! Only `bt_swift_hosts_connect` stays (Swift owns `TerminalSession`
//! creation and the `CitadelSSHClient` dependency).

#![cfg(target_os = "ios")]

use std::ffi::c_char;

extern "C" {
    /// Fire Swift's existing connect-flow orchestration for the given
    /// host id. Swift owns the toaster / mismatch dialog / terminal
    /// push behaviour from here on out.
    pub fn bt_swift_hosts_connect(id: *const c_char);
}
