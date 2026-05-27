//! iOS UI layer for BedTerm.
//!
//! Runtime surface: a `UIViewController` subclass that hosts two text-input
//! surfaces (a Metal-backed `BtIosMetalInputView` and a UITextView composer),
//! a persistent keybar, and a DEBUG-only HUD. Focus is routed through
//! `BtIosKeyboardCoordinator`.
//!
//! **See [`ARCHITECTURE.md`](../ARCHITECTURE.md)** at the crate root for the
//! full ownership graph, lifecycle ordering rules, threading model, and FFI
//! surface map. Every raw-pointer ivar in the crate has a field-level
//! SAFETY comment documenting its weak-pointer contract; the architecture
//! doc is the index for those contracts.
//!
//! ## Host-testability
//!
//! Pure-logic modules (`color`, `input_mode`, `scroll_physics`,
//! `block_header`, `block_panel_style`, `display_mode`, `prompt_context`,
//! `geometry`, `metal_selection_layer::SelectionRange`, and most of
//! `block_list/*`) are not gated, so `cargo test -p bedterm_ios` on a
//! macOS host runs their unit tests. UIKit/Metal-coupled modules
//! (`metal_view`, `vc`, `coordinator`, `keybar`, …) are gated on
//! `target_os = "ios"` because they instantiate Objective-C classes that
//! only exist on the device/simulator. The objc2 crates themselves still
//! compile on macOS hosts (the bindings just don't link UIKit symbols),
//! which lets the pure modules use `msg_send!` / `AnyObject` types as
//! type-system glue without forcing a target gate on the whole crate.
//!
//! ## FFI layout
//!
//! All `bt_ios_*` C exports live under [`ffi`], split per namespace
//! (`ffi::vc`, `ffi::view`). The SSH bridge exports stay co-located
//! with [`ssh_bridge`]. No `#[no_mangle]` items live in this file.

// Pure-logic modules — compile on every host so their #[test]s run
// without an iOS simulator.
mod block_header;
mod block_panel_style;
mod color;
mod design_system;
mod display_mode;
mod geometry;
mod input_mode;
mod metal_selection_layer;
mod prompt_context;
mod scroll_physics;

// `block_list` is mostly pure (layout/scroll/sticky/selection state).
// The `BtIosBlockListViewController` UIKit class inside `block_list::mod`
// is gated internally on iOS so the pure submodules stay host-testable.
mod block_list;

// UIKit / Metal-coupled modules — instantiate ObjC classes, must run on
// iOS. Gated so the crate still builds on host for unit tests.
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
mod ime_preedit_overlay;
#[cfg(target_os = "ios")]
mod keybar;
// Compile-time-embedded i18n. Tables come from
// `BedTerm/Localizable.xcstrings` via `build.rs`; runtime FFI is one
// `bt_ios_set_locale` call from Swift. Pure data — compiles and unit-
// tests on macOS host too.
mod l10n;
#[cfg(target_os = "ios")]
mod metal_cursor_layer;
#[cfg(target_os = "ios")]
mod metal_view;
// `hosts::model` is pure (no UIKit); the iOS-only VC + bridge live
// behind a `target_os = "ios"` gate inside the module.
mod hosts;
// `connect_form::model` is pure (no UIKit); the iOS-only VC + bridge
// live behind a `target_os = "ios"` gate inside the module.
mod connect_form;
mod onboarding;
#[cfg(target_os = "ios")]
mod prompt_context_chips;
// App settings persistence — owns the `NSUserDefaults` reads/writes
// that used to live in Swift's `BedTermSettings`. iOS-gated because
// `NSUserDefaults::standardUserDefaults` isn't useful from the macOS
// host unit-test runner.
#[cfg(target_os = "ios")]
mod settings_store;
// Saved-hosts persistence — owns the per-UUID Keychain blobs +
// `NSUserDefaults` order index that used to live in Swift's `HostsStore`.
// iOS-gated because the Keychain and `NSUserDefaults` aren't useful
// from the host unit-test runner.
#[cfg(target_os = "ios")]
mod hosts_store;
// Per-host SSH host-key fingerprint persistence — Swift's `HostKeyStore`
// is now a thin shim over `bt_ios_host_keys_*`. iOS-gated for the same
// reason as `hosts_store`: the Keychain isn't reachable from the macOS
// host unit-test runner.
#[cfg(target_os = "ios")]
mod host_key_store;
// Pure state machine that backs Swift's `HostsViewModel`. No UIKit deps,
// runs as host unit tests via `cargo test`. The iOS-gated FFI singleton
// lives in `ffi::hosts`.
mod hosts_vm;
#[cfg(target_os = "ios")]
mod settings_vc;
// `ssh_bridge` exposes a vtable + result enum the Swift side fills in
// at runtime. Several items (the result-code variants, the Swift
// detail-message extern) are only reached across the FFI boundary, so
// the dead-code lint can't see their consumers — silence it here
// rather than touching the module itself.
#[allow(dead_code)]
#[cfg(target_os = "ios")]
mod ssh_bridge;
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

/// Opaque callback type fired when the in-VC back button is tapped.
#[cfg(target_os = "ios")]
pub type BtIosBackCallback = unsafe extern "C" fn(ctx: *mut std::ffi::c_void);
