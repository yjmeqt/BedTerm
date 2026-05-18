//! Process-wide `cosmic_text::FontSystem`. Constructed lazily on first use,
//! reused for the lifetime of the process. The underlying `fontdb` scan is
//! ~200–500 ms on iOS, so this is the only place the cost is paid.

use cosmic_text::FontSystem;
use std::sync::{Mutex, OnceLock};

static FONT_SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();

/// Run `f` against the shared FontSystem. Blocks if another caller is
/// rasterising. Don't hold the guard across rasterization that itself
/// might recurse into the FontSystem — currently no such code path exists.
pub fn with_font_system<R, F: FnOnce(&mut FontSystem) -> R>(f: F) -> R {
    let lock = FONT_SYSTEM.get_or_init(|| Mutex::new(FontSystem::new()));
    let mut guard = lock.lock().expect("FontSystem mutex poisoned");
    f(&mut *guard)
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
        let found = with_font_system(|fs| {
            fs.db().faces().any(|f| {
                let names = f
                    .families
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>();
                names.iter().any(|n| {
                    n.contains("Menlo")
                        || n.contains("Monaco")
                        || n.contains("Courier")
                        || n.contains("SF Mono")
                })
            })
        });
        assert!(found, "no system monospace font discoverable via fontdb");
    }
}
