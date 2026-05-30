//! iOS UI layer for BedTerm.
//!
//! Thin UIKit/Metal layer that depends on [`bedterm_app`] for all pure
//! application logic (models, state machines, SSH client, design tokens,
//! i18n). See `bedterm-app/src/lib.rs` for the pure-logic crate.
//!
//! **See [`ARCHITECTURE.md`](../ARCHITECTURE.md)** at the crate root for the
//! full ownership graph, lifecycle ordering rules, threading model, and FFI
//! surface map.
//!
//! ## FFI layout
//!
//! All `bt_ios_*` C exports live under [`ffi`], split per namespace
//! (`ffi::vc`, `ffi::view`). The SSH bridge exports live in
//! `bedterm_app::ssh_bridge`. No `#[no_mangle]` items live in this file.

// ── Split modules: pure parts in bedterm-app, iOS parts here ──

// design_system: colors (pure tokens in bedterm-app, UIColor factories here)
mod design_system;

// input_mode: ModeState (iOS-gated) stays here; InputMode enum in bedterm-app
#[cfg(target_os = "ios")]
mod input_mode;

// metal_selection_layer: MetalSelectionLayer (CAShapeLayer) stays here;
// SelectionRange in bedterm_app::selection_range
#[cfg(target_os = "ios")]
mod metal_selection_layer;

// toaster: BtIosToasterView + card factory stay here; ToastKind/ToastAction
// pure types in bedterm_app::toaster
#[cfg(target_os = "ios")]
mod toaster;

// block_list: BtIosBlockListViewController stays here; layout/scroll/sticky/
// selection pure math in bedterm_app::block_list
mod block_list;

// hosts: hosts_vc stays here; model in bedterm_app::hosts::model
mod hosts;

// connect_form: connect_form_vc stays here; model in bedterm_app::connect_form::model
mod connect_form;

// onboarding: coordinator + VCs stay here; state in bedterm_app::onboarding::state
mod onboarding;

// ── UIKit / Metal-coupled modules (iOS only) ──
#[cfg(target_os = "ios")]
mod a11y;
#[cfg(target_os = "ios")]
mod action_chip;
#[cfg(target_os = "ios")]
mod block_list_composer;
#[cfg(target_os = "ios")]
mod composer_text_view;
#[cfg(target_os = "ios")]
mod connecting_overlay;
#[cfg(target_os = "ios")]
mod coordinator;
#[cfg(target_os = "ios")]
mod debug_hud;
#[cfg(target_os = "ios")]
mod disconnect_banner;
#[cfg(target_os = "ios")]
mod dpad;
#[cfg(target_os = "ios")]
mod host_key_mismatch_vc;
#[cfg(target_os = "ios")]
mod hosts_flow_controller;
#[cfg(target_os = "ios")]
mod hosts_store;
#[cfg(target_os = "ios")]
mod ime_preedit_overlay;
#[cfg(target_os = "ios")]
mod keybar;
#[cfg(target_os = "ios")]
mod metal_cursor_layer;
#[cfg(target_os = "ios")]
mod metal_view;
#[cfg(target_os = "ios")]
mod prompt_context_chips;
#[cfg(target_os = "ios")]
pub mod root_coordinator;
#[cfg(target_os = "ios")]
mod settings_store;
#[cfg(target_os = "ios")]
mod settings_vc;
#[cfg(target_os = "ios")]
mod terminal_palette;
#[cfg(target_os = "ios")]
mod text_input;
#[cfg(target_os = "ios")]
mod tokens;
#[cfg(target_os = "ios")]
mod vc;

// FFI namespace — per-namespace `#[no_mangle]` submodules. iOS-gated to
// match the modules they front.
#[cfg(target_os = "ios")]
mod ffi;

// Higher-level terminal session FFI: combines SSH lifecycle into a single
// handle-based abstraction (connect → open_shell → byte pump → disconnect).
#[cfg(target_os = "ios")]
pub mod terminal_session;

/// Opaque callback type fired when the in-VC back button is tapped.
///
/// Defined unconditionally (no `#[cfg(target_os = "ios")]`) so cbindgen
/// emits the typedef in the generated C header — Swift tests reference
/// it by name (e.g. `IosTerminalFFITests.onSendCallback: BtIosOnSendCallback`).
pub type BtIosBackCallback = unsafe extern "C" fn(ctx: *mut std::ffi::c_void);

/// PTY-byte sink callback installed on `BtIosMetalInputView`. Swift
/// tests reference this typedef by name; see `BtIosBackCallback` for
/// the cbindgen-visibility rationale.
pub type BtIosOnSendCallback =
    unsafe extern "C" fn(ctx: *mut std::ffi::c_void, bytes: *const u8, len: usize);

/// Grid-resize callback fired from `BtIosMetalInputView::layoutSubviews`.
/// Swift tests reference this typedef by name; see `BtIosBackCallback`
/// for the cbindgen-visibility rationale.
pub type BtIosOnResizeCallback =
    unsafe extern "C" fn(ctx: *mut std::ffi::c_void, cols: u16, rows: u16);
