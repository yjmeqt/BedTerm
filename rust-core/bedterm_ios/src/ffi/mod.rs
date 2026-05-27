//! Per-namespace FFI exports for `bedterm_ios`.
//!
//! All `bt_ios_*` C exports live in here, split by responsibility:
//!
//! - [`vc`]   — view controller construction / release.
//! - [`view`] — `BtIosMetalInputView` byte feed, sink + resize
//!   callbacks, grid metrics.
//!
//! The SSH bridge exports (`bt_ios_register_ssh_bridge`,
//! `bt_ios_ssh_bridge_release`, `bt_ssh_release_message`) stay co-located
//! with their owning module in [`crate::ssh_bridge`] — the
//! `#[no_mangle]` symbols are picked up by the linker regardless of
//! module path, so there's no value in funnelling them through a
//! re-export here.

#![cfg(target_os = "ios")]

pub mod connect_form;
pub mod connect_form_vm;
pub mod host_keys;
pub mod hosts;
pub mod onboarding;
pub mod settings;
pub mod vc;
pub mod view;
