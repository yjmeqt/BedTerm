//! Process-wide `cosmic_text::FontSystem`. Constructed lazily on first use,
//! reused for the lifetime of the process.
//!
//! On iOS the app sandbox blocks fontdb's default scan of
//! `/System/Library/Fonts` — the sandbox sees zero faces and cosmic-text
//! panics with "no default font found" the first time it tries to shape
//! text. We sidestep that by embedding a monospace font (JetBrains Mono)
//! directly into the binary via `include_bytes!` and registering it into
//! fontdb at FontSystem construction. The bundled font is what
//! `Family::Name("JetBrains Mono")` resolves to in `glyph_raster.rs`.
//!
//! CJK + emoji fonts can be added by extending `BUNDLED_FONTS` — but for
//! the first cosmic-text iOS landing we ship monospace-only and accept
//! tofu for non-Latin codepoints until those bundles arrive.

use cosmic_text::FontSystem;
use std::sync::{Mutex, OnceLock};

/// Primary monospace face we ship inside the binary. Apache-2.0 licensed
/// (JetBrains Mono v2.x). The family name registered in fontdb is
/// `"JetBrains Mono"` (whatever the file's `name` table records — the
/// JetBrainsMono distribution sets exactly that).
const JETBRAINS_MONO_REGULAR: &[u8] = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");

/// All fonts to register at FontSystem startup, in the order callers
/// prefer them. Extend this list to bundle CJK / emoji coverage.
const BUNDLED_FONTS: &[&[u8]] = &[JETBRAINS_MONO_REGULAR];

static FONT_SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();

/// Run `f` against the shared FontSystem. Blocks if another caller is
/// rasterising. Don't hold the guard across rasterization that itself
/// might recurse into the FontSystem — currently no such code path exists.
pub(crate) fn with_font_system<R, F: FnOnce(&mut FontSystem) -> R>(f: F) -> R {
    let lock = FONT_SYSTEM.get_or_init(|| Mutex::new(build_font_system()));
    let mut guard = lock.lock().expect("FontSystem mutex poisoned");
    f(&mut guard)
}

fn build_font_system() -> FontSystem {
    // `FontSystem::new()` runs `Database::load_system_fonts()` internally.
    // On macOS host this picks up Menlo/SF Mono/etc.; on iOS sandbox it
    // adds zero faces. Either way, we then load the bundled fonts on top —
    // they are the authoritative cascade root.
    let mut fs = FontSystem::new();
    let db = fs.db_mut();
    for &bytes in BUNDLED_FONTS {
        // `load_font_data` parses the bytes; on parse failure it silently
        // adds zero faces. We don't fail the whole startup over one bad
        // bundle — system fonts (if any) still provide fallback.
        db.load_font_data(bytes.to_vec());
    }
    fs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_monospace_is_loaded() {
        let count = with_font_system(|fs| fs.db().len());
        assert!(
            count > 0,
            "FontSystem reports zero faces even after bundled-font load"
        );
    }

    #[test]
    fn jetbrains_mono_family_is_discoverable() {
        let found = with_font_system(|fs| {
            fs.db().faces().any(|f| {
                f.families
                    .iter()
                    .any(|(name, _)| name.contains("JetBrains Mono"))
            })
        });
        assert!(
            found,
            "JetBrains Mono not discoverable in fontdb after bundling"
        );
    }

    #[test]
    fn bundled_face_is_flagged_monospace() {
        let found = with_font_system(|fs| {
            fs.db().faces().any(|f| {
                f.monospaced
                    && f.families
                        .iter()
                        .any(|(name, _)| name.contains("JetBrains Mono"))
            })
        });
        assert!(
            found,
            "bundled JetBrains Mono face is not flagged monospaced — \
             check the panose/post table in the bundled .ttf"
        );
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
