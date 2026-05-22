// M3 wires these into `header_band::emit_header`; silence clippy until
// then so M2 lands as a self-contained, fully-tested module.
#![allow(dead_code)]

//! Proportional (UI) text rasterization via cosmic-text + swash.
//!
//! Mirrors `glyph_raster::rasterize` but
//! - shapes a whole grapheme cluster (one or more codepoints) rather
//!   than a single `char`, since proper text shaping in CJK / emoji
//!   needs multi-codepoint context, and
//! - accepts arbitrary `font_size_px` per call so the same module
//!   serves both subheadline and caption2 sizes (and adapts on
//!   Dynamic Type changes).
//!
//! The system UI font (`SF Pro` on iOS) is discovered through
//! cosmic-text's fontdb cascade; CJK / Cyrillic / Hiragana fall through
//! to fontdb's fallback chain automatically.

use cosmic_text::{Attrs, AttrsList, BufferLine, Family, LineEnding, Shaping, Wrap};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;

use super::font_system::with_font_system;
use super::glyph_raster::RasterizedGlyph;

/// One shaped run from `shape_advances` — a grapheme + its horizontal
/// advance width in pixels.
pub struct ShapedGlyph {
    /// UTF-8 slice borrowed from the input string. Caller is expected
    /// to hold the input alive for as long as it uses these.
    /// M3 uses this when emitting per-glyph quads; currently only
    /// `advance_px` is consumed by `fit_prefix`.
    #[allow(dead_code)]
    pub text: String,
    /// Horizontal advance in pixels at the requested size.
    pub advance_px: f32,
}

/// Shape `text` at `font_size_px` and return per-glyph advances. Used
/// by the header band to position glyphs and compute truncation
/// boundaries.
pub fn shape_advances(text: &str, font_size_px: f32) -> Vec<ShapedGlyph> {
    if text.is_empty() {
        return Vec::new();
    }
    with_font_system(|fs| {
        let attrs = Attrs::new()
            .family(Family::Name("SF Pro"))
            .family(Family::SansSerif);
        let attrs_list = AttrsList::new(attrs);
        let mut line = BufferLine::new(text, LineEnding::None, attrs_list, Shaping::Advanced);
        let layout = line.layout(fs, font_size_px, None, Wrap::None, None, 8);
        let mut out: Vec<ShapedGlyph> = Vec::new();
        if let Some(run) = layout.first() {
            for g in run.glyphs.iter() {
                let start = g.start.min(text.len());
                let end = g.end.min(text.len()).max(start);
                let slice = &text[start..end];
                out.push(ShapedGlyph {
                    text: slice.to_string(),
                    advance_px: g.w,
                });
            }
        }
        out
    })
}

/// Rasterize a single grapheme `s` (typically one codepoint) at
/// `font_size_px` from the UI font cascade. Returns `None` for empty /
/// whitespace graphemes with no inked pixels.
pub fn rasterize_ui(s: &str, font_size_px: f32) -> Option<RasterizedGlyph> {
    if s.is_empty() {
        return None;
    }
    with_font_system(|fs| {
        let attrs = Attrs::new()
            .family(Family::Name("SF Pro"))
            .family(Family::SansSerif);
        let attrs_list = AttrsList::new(attrs);
        let mut line = BufferLine::new(s, LineEnding::None, attrs_list, Shaping::Advanced);
        let layout = line.layout(fs, font_size_px, None, Wrap::None, None, 8);
        let lg = layout.first()?.glyphs.first()?;
        let font_id = lg.font_id;
        let glyph_id = lg.glyph_id;

        let font = fs.get_font(font_id)?;
        let font_ref = font.as_swash();

        let mut ctx = ScaleContext::new();
        let mut scaler = ctx.builder(font_ref).size(font_size_px).hint(true).build();

        let image = Render::new(&[
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::ColorOutline(0),
            Source::Outline,
        ])
        .format(Format::Alpha)
        .render(&mut scaler, glyph_id)?;

        if image.placement.width == 0 || image.placement.height == 0 {
            return None;
        }
        let w = image.placement.width;
        let h = image.placement.height;

        // Format::Alpha always yields a single-channel Content::Mask. We
        // expand to BGRA8 so the atlas's upload path is uniform with
        // colour glyphs.
        debug_assert!(matches!(image.content, Content::Mask));
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for &a in image.data.iter() {
            bgra.push(a);
            bgra.push(a);
            bgra.push(a);
            bgra.push(a);
        }
        Some(RasterizedGlyph {
            pixels: bgra,
            width: w,
            height: h,
            left: image.placement.left,
            top: image.placement.top,
            is_color: false,
            font_id,
        })
    })
}

/// Picks the largest prefix of `glyphs` whose total advance fits inside
/// `max_w` when followed by an ellipsis run of width `ellipsis_w`.
///
/// Returns `(take, needs_ellipsis, prefix_width)`:
/// - `take`: number of glyphs from `glyphs` to keep
/// - `needs_ellipsis`: whether the ellipsis run should be appended
/// - `prefix_width`: total advance of the kept prefix (without ellipsis)
///
/// Pure arithmetic — split out from the draw path so it can be unit
/// tested without touching cosmic-text / swash.
pub fn fit_prefix(glyphs: &[ShapedGlyph], ellipsis_w: f32, max_w: f32) -> (usize, bool, f32) {
    let total_w: f32 = glyphs.iter().map(|g| g.advance_px).sum();
    if total_w <= max_w {
        return (glyphs.len(), false, total_w);
    }
    let mut acc: f32 = 0.0;
    let mut take = 0usize;
    for (i, g) in glyphs.iter().enumerate() {
        if acc + g.advance_px + ellipsis_w > max_w {
            break;
        }
        acc += g.advance_px;
        take = i + 1;
    }
    (take, true, acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(s: &str, w: f32) -> ShapedGlyph {
        ShapedGlyph {
            text: s.to_string(),
            advance_px: w,
        }
    }

    #[test]
    fn shape_advances_returns_non_empty_for_ascii() {
        let v = shape_advances("hello", 14.0);
        assert_eq!(v.len(), 5);
        for s in &v {
            assert!(s.advance_px > 0.0);
        }
    }

    #[test]
    fn rasterizes_ascii() {
        let r = rasterize_ui("A", 14.0).expect("A glyph");
        assert!(r.width > 0 && r.height > 0);
    }

    #[test]
    fn rasterizes_cjk_via_fallback() {
        // System font cascade on macOS hosts always covers CJK via
        // PingFang / Hiragino. If this assertion ever flakes the
        // fontdb fallback chain has regressed and we'd want to know.
        let r = rasterize_ui("中", 14.0).expect("CJK glyph via fontdb fallback");
        assert!(r.width > 0 && r.height > 0);
    }

    #[test]
    fn fit_prefix_returns_full_run_when_fits() {
        let v = vec![g("a", 10.0), g("b", 10.0), g("c", 10.0)];
        let (take, ellip, w) = fit_prefix(&v, 5.0, 100.0);
        assert_eq!(take, 3);
        assert!(!ellip);
        assert!((w - 30.0).abs() < 0.01);
    }

    #[test]
    fn fit_prefix_truncates_with_ellipsis() {
        let v = vec![g("a", 10.0), g("b", 10.0), g("c", 10.0), g("d", 10.0)];
        // max 25 px, ellipsis 5 → can fit 2 glyphs (20) + ellipsis (5) = 25.
        let (take, ellip, w) = fit_prefix(&v, 5.0, 25.0);
        assert_eq!(take, 2);
        assert!(ellip);
        assert!((w - 20.0).abs() < 0.01);
    }

    #[test]
    fn fit_prefix_handles_too_narrow() {
        let v = vec![g("a", 10.0), g("b", 10.0)];
        // max 4 px, ellipsis 5 → nothing fits.
        let (take, ellip, w) = fit_prefix(&v, 5.0, 4.0);
        assert_eq!(take, 0);
        assert!(ellip);
        assert!((w - 0.0).abs() < 0.01);
    }
}
