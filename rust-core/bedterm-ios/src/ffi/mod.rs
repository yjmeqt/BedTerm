//! FFI exports called from Swift.
//!
//! Only `settings` has a `#[no_mangle]` export (`bt_ios_settings_onboarding_completed`).
//! The main entry points live in `root_coordinator.rs`.

pub mod settings;
pub(crate) mod vc;
