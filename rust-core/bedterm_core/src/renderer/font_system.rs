//! Process-wide `cosmic_text::FontSystem`. Constructed lazily on first
//! use, reused for the lifetime of the process.
//!
//! ## Font cascade
//!
//! The terminal primary font is **always host-supplied** — Swift wires
//! SF Mono (via `CTFontCopyTable` sfnt reassembly) or Menlo through
//! `bt_font_register_terminal_face` at `MetalEnvironment.init()`. No
//! third-party monospace is bundled.
//!
//! Fallback faces (CJK, color emoji) are still embedded via
//! `include_bytes!` because iOS doesn't expose Apple's CJK / Apple
//! Color Emoji files to sandboxed apps in a way fontdb can consume, so
//! shipping our own is the only portable option.
//!
//! ## Why no monospace fallback
//!
//! On iOS the host always provides SF Mono via
//! `UIFont.monospacedSystemFont(...)`, and Menlo as a named secondary.
//! Carrying a third option (JetBrains Mono, Fira Code, …) just adds
//! binary weight and a font most users would never see — host fonts
//! are guaranteed available, so we trust them.
//!
//! If neither registers (e.g. the iOS APIs return something we can't
//! reassemble), shape calls will fall through to whatever fontdb's
//! `Database::load_system_fonts` picked up — on iOS that's empty and
//! the rasterizer will return `None` for ASCII rather than rendering
//! something ugly. The host integration is responsible for ensuring
//! one of SF Mono / Menlo lands before the first frame.

use cosmic_text::fontdb::Source;
use cosmic_text::FontSystem;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// Family name `glyph_raster.rs` requests at shape time. Set by Swift
/// via `bt_font_register_terminal_face` during `MetalEnvironment.init()`
/// — sourced from fontdb's `name`-table parse, not from the iOS-side
/// `CTFontCopyFamilyName` (which can disagree for system UI fonts).
///
/// `RwLock<Option<String>>` so the initial pre-bootstrap state is
/// distinguishable from "set to empty" — `terminal_family()` returns
/// the in-band sentinel `MISSING_TERMINAL_FAMILY` in that case, which
/// no real font matches, so cosmic-text's cascade walks to the next
/// available family rather than silently rendering as a wrong font.
static TERMINAL_FAMILY: RwLock<Option<String>> = RwLock::new(None);

/// Sentinel family returned by `terminal_family()` when no host face
/// has been registered yet. Chosen to never match any real font name
/// so `Family::Name(...)` lookups miss cleanly and cosmic-text falls
/// through to its standard cascade.
const MISSING_TERMINAL_FAMILY: &str = "__bedterm-no-host-font__";

/// Family name shape calls should request as the primary monospace.
/// Returns a heap `String` (rather than `&'static str`) so callers can
/// pass `Family::Name(&s)` without lifetime gymnastics.
pub(crate) fn terminal_family() -> String {
    TERMINAL_FAMILY
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| MISSING_TERMINAL_FAMILY.to_string())
}

/// Load host-supplied TTF/TTC/OTF bytes into the shared fontdb and
/// promote the first newly-added face's family name to the primary
/// terminal family. Returns the resolved family name on success.
///
/// The family is read back from fontdb's parsed sfnt `name` table —
/// not from whatever name the iOS side reported. This matters because
/// `UIFont.monospacedSystemFont(...)`'s CTFont surface name is the
/// internal token `.AppleSystemUIFontMonospaced`, while the actual
/// sfnt blob declares `SF Mono`. cosmic-text's `Family::Name(...)`
/// matcher only sees fontdb's view, so reading the name back from
/// fontdb after load is the only way to guarantee the request string
/// at shape time agrees with what the database stored.
pub(crate) fn register_terminal_face(bytes: Vec<u8>) -> Option<String> {
    with_font_system(|fs| {
        let db = fs.db_mut();
        // `Source::Binary` returns the face IDs that were actually
        // added (a .ttc can spawn many; .ttf usually one). We want
        // the first English-language family name from face #0.
        let ids = db.load_font_source(Source::Binary(Arc::new(bytes)));
        let chosen = ids
            .into_iter()
            .find_map(|id| db.face(id).and_then(|face| face.families.first().cloned()))
            .map(|(name, _)| name);
        if let Some(name) = chosen.as_ref() {
            if let Ok(mut g) = TERMINAL_FAMILY.write() {
                *g = Some(name.clone());
            }
        }
        chosen
    })
}

/// Register a host-supplied auxiliary face (Menlo-Bold, Menlo-Italic,
/// Menlo-BoldItalic, …) into fontdb **without** touching the primary
/// `TERMINAL_FAMILY`. cosmic-text matches `Attrs::weight` / `style`
/// against fontdb after picking the family — so loading these here
/// lets `Family::Name("Menlo")` + `Weight::BOLD` resolve to the bold
/// face at shape time. Returns `true` if at least one face was added.
pub(crate) fn register_aux_face(bytes: Vec<u8>) -> bool {
    with_font_system(|fs| {
        let ids = fs
            .db_mut()
            .load_font_source(Source::Binary(Arc::new(bytes)));
        !ids.is_empty()
    })
}

/// CJK fallback. OFL licensed (Noto Sans Mono CJK SC Regular, ~16 MB).
/// Covers Simplified Chinese, Japanese, and Korean ideographs;
/// cosmic-text's fontdb cascade picks this up automatically when the
/// host primary lacks coverage for a codepoint.
const NOTO_SANS_MONO_CJK_SC: &[u8] = include_bytes!("../../assets/NotoSansMonoCJKsc-Regular.otf");

/// Color emoji fallback. OFL licensed (Noto Color Emoji, ~10 MB; CBDT
/// bitmap strikes consumed by swash's `Source::ColorBitmap` path). The
/// rasterizer's source cascade renders colour glyphs through this when
/// `glyph_raster.rs` sees `Content::Color` from swash.
const NOTO_COLOR_EMOJI: &[u8] = include_bytes!("../../assets/NotoColorEmoji.ttf");

/// Bundled non-primary faces — CJK + colour emoji. The host primary
/// (SF Mono / Menlo) is registered separately by Swift at startup.
const BUNDLED_FALLBACKS: &[&[u8]] = &[NOTO_SANS_MONO_CJK_SC, NOTO_COLOR_EMOJI];

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
    // adds zero faces. Either way, we then load the bundled fallback
    // fonts on top — host monospace registration is Swift's job.
    let mut fs = FontSystem::new();
    let db = fs.db_mut();
    for &bytes in BUNDLED_FALLBACKS {
        // `load_font_data` parses the bytes; on parse failure it silently
        // adds zero faces. We don't fail the whole startup over one bad
        // bundle — host fonts (if any) still provide primary coverage.
        db.load_font_data(bytes.to_vec());
    }
    fs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallbacks_are_loaded() {
        let count = with_font_system(|fs| fs.db().len());
        assert!(
            count > 0,
            "FontSystem reports zero faces even after bundled-fallback load"
        );
    }

    #[test]
    fn cjk_fallback_is_discoverable() {
        let found = with_font_system(|fs| {
            fs.db().faces().any(|f| {
                f.families
                    .iter()
                    .any(|(name, _)| name.contains("Noto Sans Mono CJK"))
            })
        });
        assert!(found, "Noto Sans Mono CJK not discoverable in fontdb");
    }

    #[test]
    fn register_terminal_face_promotes_family() {
        // Save/restore TERMINAL_FAMILY around the test so parallel test
        // threads (`rasterizes_cjk_via_font_fallback` in particular,
        // which assumes cosmic-text's default cascade) don't see a
        // primary family promoted to the CJK fallback.
        let prior = TERMINAL_FAMILY.read().ok().and_then(|g| g.clone());
        let bytes = super::NOTO_SANS_MONO_CJK_SC.to_vec();
        let resolved = register_terminal_face(bytes).expect("register failed");
        assert!(
            resolved.contains("Noto Sans Mono CJK"),
            "resolved family `{resolved}` doesn't match the loaded face"
        );
        assert_eq!(terminal_family(), resolved);
        if let Ok(mut g) = TERMINAL_FAMILY.write() {
            *g = prior;
        }
    }

    #[test]
    fn font_system_is_a_singleton_across_calls() {
        let (len1, ptr1) = with_font_system(|fs| (fs.db().len(), fs.db() as *const _ as usize));
        let (len2, ptr2) = with_font_system(|fs| (fs.db().len(), fs.db() as *const _ as usize));
        assert!(
            len1 <= len2,
            "fontdb shrank between calls — was the FontSystem rebuilt?"
        );
        assert_eq!(
            ptr1, ptr2,
            "db() pointer changed between calls — singleton not honoured"
        );
    }
}
