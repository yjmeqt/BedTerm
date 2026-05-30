//! Pure application logic for BedTerm — models, state machines, SSH client,
//! design tokens, i18n, and shared types.
//!
//! This crate has **no UIKit, Metal, or CoreFoundation dependencies**. It
//! compiles and tests on macOS host without an iOS simulator.
//!
//! The iOS UI layer ([`bedterm_ios`]) depends on this crate for all its
//! data types and pure-logic state machines.
//!
//! ## Host-testability
//!
//! All modules are unconditional — `cargo test -p bedterm-app` on a macOS
//! host runs every unit test. The `objc2` base crate (used only for
//! `Encode`/`Encoding` trait impls on geometry types) compiles on all
//! platforms with an ObjC runtime.
//!
//! ## FFI
//!
//! The only `#[no_mangle]` export here is `bt_ios_set_locale` (called from
//! Swift). All other FFI exports were internal Rust-to-Rust calls that have
//! been converted to normal `pub(crate)` functions.

// Allow dead_code on macOS host where iOS-gated consumers in bedterm-ios
// aren't compiled.
#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

// ── Top-level pure modules ──
pub mod block_header;
pub mod block_panel_style;
pub mod color;
pub mod credential;
mod display_mode;
pub mod geometry;
pub mod input_mode;
pub mod l10n;
pub mod net_util;
pub mod prompt_context;
pub mod pty;
pub mod scroll_physics;
pub mod selection_range;
pub mod shell_integration;
pub mod ssh_bridge;

// ── Subdirectory modules ──
pub mod block_list;
pub mod connect_form;
pub mod design_system;
pub mod hosts;
pub mod onboarding;
pub mod ssh_client;

// ── State machines ──
pub mod connect_form_vm;
pub mod hosts_vm;
