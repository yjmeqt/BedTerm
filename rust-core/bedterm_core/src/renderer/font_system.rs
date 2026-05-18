//! Process-wide `cosmic_text::FontSystem`. Constructed lazily on first use,
//! reused for the lifetime of the process. The underlying `fontdb` scan is
//! ~200–500 ms on iOS, so this is the only place the cost is paid.

use cosmic_text::FontSystem;
use std::sync::{Mutex, OnceLock};

static FONT_SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();

/// Run `f` against the shared FontSystem. Blocks if another caller is
/// rasterising. Don't hold the guard across rasterization that itself
/// might recurse into the FontSystem — currently no such code path exists.
pub(crate) fn with_font_system<R, F: FnOnce(&mut FontSystem) -> R>(f: F) -> R {
    let lock = FONT_SYSTEM.get_or_init(|| Mutex::new(FontSystem::new()));
    let mut guard = lock.lock().expect("FontSystem mutex poisoned");
    f(&mut guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_system_initialises() {
        let count = with_font_system(|fs| fs.db().len());
        assert!(count > 0, "fontdb should have discovered at least one face");
    }

    #[test]
    fn at_least_one_monospace_face_is_discoverable() {
        // fontdb sets `monospaced` from the OS/2 panose + post table — the
        // authoritative signal. Substring matching family names is unreliable
        // (a face named "Courier New Decorative" can be proportional).
        let found = with_font_system(|fs| fs.db().faces().any(|f| f.monospaced));
        assert!(found, "no system monospace font discoverable via fontdb");
    }

    #[test]
    fn font_system_is_a_singleton_across_calls() {
        let (len1, ptr1) = with_font_system(|fs| (fs.db().len(), fs.db() as *const _ as usize));
        let (len2, ptr2) = with_font_system(|fs| (fs.db().len(), fs.db() as *const _ as usize));
        assert_eq!(
            len1, len2,
            "fontdb length changed between calls — was the FontSystem rebuilt?"
        );
        assert_eq!(
            ptr1, ptr2,
            "db() pointer changed between calls — singleton not honoured"
        );
    }
}
