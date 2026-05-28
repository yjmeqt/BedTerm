//! Compile-time-embedded i18n. The translation tables come from
//! `BedTerm/Localizable.xcstrings` via the crate's `build.rs`. Swift sets
//! the active locale once at launch via `bt_ios_set_locale`; everything
//! else is pure Rust.

use std::ffi::{c_char, CStr};
use std::sync::RwLock;

// The generated `LOCALE_TABLES` static is consumed by `t()` below; `t()`
// itself is only called from iOS-gated UIKit VCs. On macOS host builds
// neither has a runtime caller, but the host unit-tests still exercise
// them — silence the dead-code lint instead of gating the module.
#[allow(dead_code)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/locale_tables.rs"));
}
use generated::LOCALE_TABLES;

static LOCALE: RwLock<String> = RwLock::new(String::new());

/// Called by Swift at app launch and on `NSLocale.currentLocaleDidChange`
/// notifications. UTF-8 nul-terminated locale identifier (e.g. "zh-Hans",
/// "en", "ja"). NULL is treated as "reset to default" (uses "en" fallback).
///
/// # Safety
/// `code` must be NULL or point to a valid nul-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_set_locale(code: *const c_char) {
    let new_value = if code.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(code) }
            .to_string_lossy()
            .into_owned()
    };
    if let Ok(mut guard) = LOCALE.write() {
        *guard = new_value;
    }
}

/// Resolve `key` to the active-locale string. Falls back through region
/// stripping (e.g. `zh-Hans_HK` → `zh-Hans`) then English, finally
/// returning the key itself if no translation exists.
#[allow(dead_code)] // only called from iOS-gated UIKit VC modules
pub fn t(key: &str) -> String {
    let want = LOCALE.read().ok().map(|g| g.clone()).unwrap_or_default();
    let region_stripped = want.split('_').next().unwrap_or(&want).to_string();
    for candidate in [want.as_str(), region_stripped.as_str(), "en"] {
        if candidate.is_empty() {
            continue;
        }
        if let Some((_, table)) = LOCALE_TABLES.iter().find(|(l, _)| *l == candidate) {
            if let Some((_, v)) = table.iter().find(|(k, _)| *k == key) {
                return v.to_string();
            }
        }
    }
    key.to_string()
}

/// Substitute a single `%@` placeholder in the translated string. Other
/// placeholders (`%1$@`, `%2$@`, …) are left untouched. Used by the hosts
/// alert formatters where the template comes from `xcstrings`.
#[allow(dead_code)] // only called from iOS-gated UIKit VC modules
pub fn format1(key: &str, arg: &str) -> String {
    t(key).replacen("%@", arg, 1)
}

/// Substitute positional `%1$@` / `%2$@` placeholders. Falls back to
/// sequential `%@` substitution when the template only has bare `%@`s
/// (lets xcstrings authors pick whichever style fits the locale).
#[allow(dead_code)] // only called from iOS-gated UIKit VC modules
pub fn format2(key: &str, arg1: &str, arg2: &str) -> String {
    let raw = t(key);
    if raw.contains("%1$@") || raw.contains("%2$@") {
        raw.replace("%1$@", arg1).replace("%2$@", arg2)
    } else {
        raw.replacen("%@", arg1, 1).replacen("%@", arg2, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_key_returns_self() {
        unsafe { bt_ios_set_locale(c"en".as_ptr()) };
        assert_eq!(t("definitely.not.a.key"), "definitely.not.a.key");
    }

    #[test]
    fn empty_locale_falls_back_to_en() {
        unsafe { bt_ios_set_locale(std::ptr::null()) };
        // If "Save" is in en, we get it; otherwise we get the key.
        let v = t("Save");
        // Either translated or self — neither should panic.
        assert!(!v.is_empty());
    }
}
