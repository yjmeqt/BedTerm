//! iOS UI layer for BedTerm — UIKit view controllers, Metal views,
//! coordinators, and the FFI surface that Swift calls.
//!
//! All pure application logic lives in `bedterm_app`. This crate is
//! gated wholesale on iOS — it does not compile on macOS host.
//!
//! See [`ARCHITECTURE.md`](../ARCHITECTURE.md) for the ownership graph.

//! ## Target
//!
//! This entire crate is gated on iOS — it does not compile on macOS host.
//! Use `cargo check -p bedterm-app` for host-side iteration.

#![cfg(target_os = "ios")]

// ── Split modules: pure parts in bedterm-app, iOS parts here ──

mod block_list;
mod connect_form;
mod design_system;
mod hosts;
mod input_mode;
mod metal_selection_layer;
mod onboarding;
mod toaster;

// ── UIKit / Metal-coupled modules ──

mod a11y;
mod action_chip;
mod block_list_composer;
mod composer_text_view;
mod connecting_overlay;
mod coordinator;
mod debug_hud;
mod disconnect_banner;
mod dpad;
mod host_key_mismatch_vc;
mod hosts_flow_controller;
mod hosts_store;
mod ime_preedit_overlay;
mod keybar;
mod metal_cursor_layer;
mod metal_view;
mod prompt_context_chips;
pub mod root_coordinator;
mod settings_store;
mod settings_vc;
mod terminal_palette;
mod text_input;
mod tokens;
mod vc;

// FFI namespace
mod ffi;

// Terminal session FFI
pub mod terminal_session;

pub type BtIosBackCallback = unsafe extern "C" fn(ctx: *mut std::ffi::c_void);
pub type BtIosOnSendCallback =
    unsafe extern "C" fn(ctx: *mut std::ffi::c_void, bytes: *const u8, len: usize);
pub type BtIosOnResizeCallback =
    unsafe extern "C" fn(ctx: *mut std::ffi::c_void, cols: u16, rows: u16);
