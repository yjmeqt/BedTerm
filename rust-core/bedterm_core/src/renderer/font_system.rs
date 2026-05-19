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
//! Emoji is still a follow-up — Apple Color Emoji isn't redistributable
//! and Noto Color Emoji renders inconsistently on iOS, so emoji codepoints
//! still produce tofu until a vetted color-emoji bundle lands.

use cosmic_text::FontSystem;
use std::sync::{Mutex, OnceLock};

/// Primary monospace face. Apache-2.0 licensed (JetBrains Mono v2.x).
/// fontdb registers it under `"JetBrains Mono"` — that name is what
/// `glyph_raster.rs` requests via `Family::Name(...)`.
const JETBRAINS_MONO_REGULAR: &[u8] = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");

/// CJK fallback. OFL licensed (Noto Sans Mono CJK SC Regular, ~16 MB).
/// Covers Simplified Chinese, Japanese, and Korean ideographs; cosmic-text's
/// fontdb cascade picks this up automatically when `JetBrains Mono` lacks
/// coverage for a codepoint. Bundling Traditional Chinese (TC) or
/// Japanese-optimised kana variants is a future opt-in.
const NOTO_SANS_MONO_CJK_SC: &[u8] =
    include_bytes!("../../assets/NotoSansMonoCJKsc-Regular.otf");

/// All fonts to register at FontSystem startup. The first entry is the
/// authoritative cascade root; subsequent entries provide automatic
/// fallback coverage for codepoints the primary lacks.
const BUNDLED_FONTS: &[&[u8]] = &[JETBRAINS_MONO_REGULAR, NOTO_SANS_MONO_CJK_SC];

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
