//! Single-codepoint rasterizer built on top of cosmic-text font discovery +
//! swash glyph scaling. Produces BGRA8 premultiplied bitmaps suitable for
//! upload into the existing Metal atlas texture.
//!
//! Font fallback is delegated entirely to cosmic-text: we shape a single-char
//! `BufferLine` with a primary `Menlo` request and let cosmic-text's cascade
//! pick the right face for CJK / emoji etc. The chosen `(font_id, glyph_id)`
//! is then fed to swash's `Render` builder, which is preferred for color
//! emoji because it honours both `sbix` (Apple Color Emoji) and `COLRv1`.
//!
//! API notes for cosmic-text 0.12.1 / swash 0.1.19 (vs the plan's example
//! code):
//! - `BufferLine::new` takes `(text, LineEnding, AttrsList, Shaping)` —
//!   no separate `metadata` arg.
//! - `Attrs` exposes `.family(...)` only; no `.monospaced(true)` builder.
//!   `Family::Name("Menlo")` followed by cosmic-text's regular fallback is
//!   sufficient for our cascade requirements.
//! - swash 0.1.19's `Render::new` accepts a slice of `Source`s. We list
//!   `ColorBitmap` first (Apple Color Emoji is sbix), then `ColorOutline`
//!   (COLR), then plain `Outline` for monochrome glyphs. `Source::Bitmap`
//!   (alpha bitmaps) is intentionally omitted — terminal monospace fonts
//!   don't ship them and including it would just slow the cascade.
//! - We render with `Format::Alpha`. `Format::Subpixel` would give us RGB
//!   triplets per pixel with positional sub-pixel filtering, which makes no
//!   sense at column boundaries in a terminal grid (and produces the
//!   `SubpixelMask` content type we'd just have to collapse anyway).

use cosmic_text::{Attrs, AttrsList, BufferLine, Family, LineEnding, Shaping, Wrap};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;

use super::font_system::with_font_system;

/// Output of [`rasterize`]. The pixel buffer is BGRA8 premultiplied and laid
/// out in row-major top-down order (`y * width + x` indexing).
//
// CT-4 wires this into the atlas; until then the only consumer is the test
// module below, which would otherwise trip dead-code lints on the struct
// and function despite the `pub` visibility.
#[allow(dead_code)]
#[derive(Debug)]
pub struct RasterizedGlyph {
    /// BGRA8 premultiplied bytes, `width * height * 4` long.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Horizontal offset (px) from cell origin to glyph's left edge.
    pub left: i32,
    /// Vertical offset (px) from cell baseline to glyph's top edge. Positive
    /// y goes up in swash, so this is normally a positive number for glyphs
    /// that sit above the baseline.
    pub top: i32,
    /// `true` when the source glyph supplied its own colors (Apple Color
    /// Emoji sbix, COLR layered outlines). Consumers use this to pick the
    /// `is_color` shader path.
    pub is_color: bool,
}

/// Rasterize a single codepoint at the given pixel size. Returns `None` when
/// no font in cosmic-text's fallback cascade actually covers the codepoint
/// (private-use scalars, lone surrogates, malformed input).
#[allow(dead_code)]
pub fn rasterize(ch: char, font_size_px: f32) -> Option<RasterizedGlyph> {
    with_font_system(|fs| {
        // Shape a single-glyph line so cosmic-text gets to walk its font
        // cascade. Monospaced primary -> CJK fallback -> Apple Color Emoji
        // are the cases we actually care about.
        let attrs = Attrs::new().family(Family::Name("Menlo"));
        let attrs_list = AttrsList::new(attrs);
        let mut text = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut text);
        let mut line = BufferLine::new(s, LineEnding::None, attrs_list, Shaping::Advanced);

        // No wrap, no width constraint, no monospace-width adjustment. We
        // only need the laid-out (font_id, glyph_id) pair.
        let layout = line.layout(fs, font_size_px, None, Wrap::None, None, 8);
        let lg = layout.first()?.glyphs.first()?;
        let font_id = lg.font_id;
        let glyph_id = lg.glyph_id;

        let font = fs.get_font(font_id)?;
        let font_ref = font.as_swash();

        // swash scaling.
        let mut ctx = ScaleContext::new();
        let mut scaler = ctx.builder(font_ref).size(font_size_px).hint(true).build();

        // Source priority:
        //   1. ColorBitmap   — Apple Color Emoji (sbix).
        //   2. ColorOutline  — COLR/CPAL color fonts (Noto Color Emoji COLRv1, etc.).
        //   3. Outline       — regular monochrome glyphs.
        // BestFit lets swash pick the closest available bitmap strike.
        let image = Render::new(&[
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::ColorOutline(0),
            Source::Outline,
        ])
        .format(Format::Alpha)
        .render(&mut scaler, glyph_id)?;

        if image.placement.width == 0 || image.placement.height == 0 {
            // A glyph with no inked pixels (space-like): nothing to upload.
            return None;
        }

        let (pixels, is_color) = match image.content {
            // 8-bit alpha mask -> opaque-white * alpha, premultiplied: each
            // output texel is (a,a,a,a).
            Content::Mask => {
                let mut out = Vec::with_capacity(image.data.len() * 4);
                for &a in &image.data {
                    out.extend_from_slice(&[a, a, a, a]);
                }
                (out, false)
            }
            // SubpixelMask is RGB-per-pixel positional AA. We don't want LCD
            // fringing on a terminal grid, so collapse to a luminance mask
            // using Rec. 601 weights and treat the result as Content::Mask.
            // (Format::Alpha shouldn't actually produce this, but handle it
            // defensively for future swash changes.)
            Content::SubpixelMask => {
                debug_assert_eq!(image.data.len() % 4, 0);
                let mut out = Vec::with_capacity(image.data.len());
                for px in image.data.chunks_exact(4) {
                    let r = px[0] as u32;
                    let g = px[1] as u32;
                    let b = px[2] as u32;
                    // 0.299 R + 0.587 G + 0.114 B (Q8 fixed-point).
                    let l = ((r * 77 + g * 150 + b * 29) >> 8) as u8;
                    out.extend_from_slice(&[l, l, l, l]);
                }
                (out, false)
            }
            // RGBA premultiplied -> BGRA byte swap.
            Content::Color => {
                debug_assert_eq!(image.data.len() % 4, 0);
                let mut out = Vec::with_capacity(image.data.len());
                for px in image.data.chunks_exact(4) {
                    out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
                (out, true)
            }
        };

        Some(RasterizedGlyph {
            pixels,
            width: image.placement.width,
            height: image.placement.height,
            left: image.placement.left,
            top: image.placement.top,
            is_color,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterizes_ascii() {
        let g = rasterize('A', 24.0).expect("ASCII 'A' must rasterise via Menlo");
        assert!(!g.pixels.is_empty(), "no pixels rendered for 'A'");
        assert_eq!(
            g.pixels.len() as u32,
            g.width * g.height * 4,
            "buffer size doesn't match width*height*4"
        );
        assert!(!g.is_color, "'A' came back flagged as color glyph");
    }

    #[test]
    fn rasterizes_cjk() {
        let g = rasterize('中', 24.0)
            .expect("CJK codepoint must rasterise via cosmic-text font cascade");
        assert!(!g.pixels.is_empty(), "no pixels rendered for '中'");
        assert_eq!(g.pixels.len() as u32, g.width * g.height * 4);
    }

    #[test]
    fn rasterizes_emoji_as_color() {
        let g = rasterize('😀', 24.0).expect("emoji must rasterise (Apple Color Emoji available)");
        assert!(!g.pixels.is_empty(), "no pixels rendered for emoji");
        assert_eq!(g.pixels.len() as u32, g.width * g.height * 4);
        assert!(
            g.is_color,
            "emoji rasterised as mask, not color -- swash didn't pick the sbix/COLR source"
        );
    }
}
