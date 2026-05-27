//! Rust→Swift bridge declarations for the W24b Hosts list VC.
//!
//! The Swift `HostsBridge.swift` `@_cdecl` shims implement these symbols.
//! Snapshot marshalling uses a single UTF-8 JSON blob (see
//! `hosts::model::parse_entries_json`) to keep the FFI seam narrow —
//! the alternative (per-field length/capacity dance) doesn't pay for
//! itself at this row count.

#![cfg(target_os = "ios")]

use std::ffi::c_char;

extern "C" {
    /// Return a +1 retained UTF-8, nul-terminated C string of the
    /// current hosts snapshot as JSON. Caller (Rust) owns the buffer
    /// and must free it via [`bt_swift_hosts_free_snapshot`]. Returns
    /// NULL if the store is locked / empty (Rust treats NULL as
    /// "empty array").
    pub fn bt_swift_hosts_snapshot_json() -> *mut c_char;

    /// Free a snapshot pointer previously returned by
    /// [`bt_swift_hosts_snapshot_json`]. NULL-safe.
    pub fn bt_swift_hosts_free_snapshot(ptr: *mut c_char);

    /// Delete the host with the given id-string (UUID string). Returns
    /// `true` if Swift handled the request (the row may already have
    /// been gone). `id` is a UTF-8 nul-terminated C string borrowed
    /// for the duration of the call.
    pub fn bt_swift_hosts_delete(id: *const c_char) -> bool;

    /// Fire Swift's existing connect-flow orchestration for the given
    /// host id. Swift owns the toaster / mismatch dialog / terminal
    /// push behaviour from here on out.
    pub fn bt_swift_hosts_connect(id: *const c_char);
}
