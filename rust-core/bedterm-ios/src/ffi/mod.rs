//! FFI exports called from Swift.
//!
//! Each module contains the minimum `#[no_mangle]` exports that Swift
//! directly calls. Internal Rust-to-Rust calls do NOT go through C FFI.

#![cfg(target_os = "ios")]

pub mod onboarding;
pub mod settings;
pub mod vc;
pub mod view;
