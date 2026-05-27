//! App settings — Rust-owned, backed by `NSUserDefaults.standard`.
//!
//! Replaces the Swift `BedTermSettings` + `SettingsBridge` pair. UI
//! consumers (the Rust Settings VC, plus the Swift hosts/terminal layer)
//! call into the C ABI exposed by [`crate::ffi::settings`]; this module
//! holds the actual `objc2-foundation` plumbing.
//!
//! No migration logic — the app has never shipped, so first-launch
//! defaults are the only thing to worry about.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSString, NSUserDefaults};

const KEY_RESERVE_TOP: &str = "settings.reserveTopSafeAreaInAltScreen";
const KEY_SHOW_BLOCKS: &str = "settings.showCommandBlocks";

fn defaults() -> Retained<NSUserDefaults> {
    NSUserDefaults::standardUserDefaults()
}

fn key(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// True iff `NSUserDefaults` holds an explicit value for `key`.
fn has_value(d: &NSUserDefaults, k: &str) -> bool {
    let s = key(k);
    let obj: Option<Retained<AnyObject>> = unsafe { msg_send![d, objectForKey: &*s] };
    obj.is_some()
}

fn read_bool(d: &NSUserDefaults, k: &str, fallback: bool) -> bool {
    if has_value(d, k) {
        let s = key(k);
        unsafe { msg_send![d, boolForKey: &*s] }
    } else {
        fallback
    }
}

fn write_bool(d: &NSUserDefaults, k: &str, value: bool) {
    let s = key(k);
    let _: () = unsafe { msg_send![d, setBool: value, forKey: &*s] };
}

pub fn reserve_top_safe_area() -> bool {
    read_bool(&defaults(), KEY_RESERVE_TOP, true)
}

pub fn set_reserve_top_safe_area(value: bool) {
    write_bool(&defaults(), KEY_RESERVE_TOP, value);
}

pub fn show_command_blocks() -> bool {
    read_bool(&defaults(), KEY_SHOW_BLOCKS, false)
}

pub fn set_show_command_blocks(value: bool) {
    write_bool(&defaults(), KEY_SHOW_BLOCKS, value);
}
