//! Glyph rasterizer abstraction. Two backends:
//!
//! - `SwashRasterizer` (cosmic-text + swash) — all platforms.
//! - `CoreTextRasterizer` (CoreText + CoreGraphics) — Apple platforms,
//!   selected automatically with `#[cfg(any(target_os = "ios", target_os = "macos"))]`.
//!
//! Both produce BGRA8 premultiplied bitmaps suitable for upload into the
//! Metal atlas texture.

use cosmic_text::{Attrs, AttrsList, BufferLine, Family, LineEnding, Shaping, Style, Weight, Wrap};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;

use super::font_system::with_font_system;

/// Output of [`GlyphRasterizer::rasterize`]. The pixel buffer is BGRA8
/// premultiplied and laid out in row-major top-down order.
#[derive(Debug)]
pub struct RasterizedGlyph {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    #[allow(dead_code)]
    pub left: i32,
    /// Vertical offset (px) from cell baseline to glyph's top edge. Positive
    /// y goes up.
    pub top: i32,
    /// `true` when the source glyph supplied its own colors (Apple Color
    /// Emoji sbix, COLR layered outlines).
    pub is_color: bool,
    #[allow(dead_code)]
    pub font_id: cosmic_text::fontdb::ID,
}

/// Monospace cell metrics derived from the resolved primary font.
#[derive(Debug, Clone, Copy)]
pub struct CellMetrics {
    pub cell_width: u32,
    pub cell_height: u32,
    pub ascent: u32,
}

/// Pluggable glyph rasterizer. One instance is owned by `GlyphAtlas`.
///
/// `logical_size_pt` is the UI-specified font size in points (before
/// display-scale multiplication). `scale` is the display's pixel scale
/// factor (DPR). The returned `RasterizedGlyph` is always in device-pixel
/// coordinates.
pub(crate) trait GlyphRasterizer {
    fn rasterize(
        &mut self,
        ch: char,
        logical_size_pt: f32,
        scale: f32,
        bold: bool,
        italic: bool,
    ) -> Option<RasterizedGlyph>;

    fn measure_cell(&mut self, logical_size_pt: f32, scale: f32) -> Option<CellMetrics>;
}

// ── Swash backend (cosmic-text + swash) ───────────────────────────────

/// Existing swash-backed rasterizer. The default on non-Apple platforms.
/// Kept on Apple platforms for tests and A/B comparison.
#[allow(dead_code)]
pub struct SwashRasterizer;

#[allow(dead_code)]
impl SwashRasterizer {
    pub fn new() -> Self {
        Self
    }
}

impl GlyphRasterizer for SwashRasterizer {
    fn rasterize(
        &mut self,
        ch: char,
        logical_size_pt: f32,
        scale: f32,
        bold: bool,
        italic: bool,
    ) -> Option<RasterizedGlyph> {
        let font_size_px = (logical_size_pt * scale).max(1.0);
        with_font_system(|fs| {
            let family = crate::renderer::font_system::terminal_family();
            let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
            let style = if italic { Style::Italic } else { Style::Normal };
            let attrs = Attrs::new()
                .family(Family::Name(&family))
                .weight(weight)
                .style(style);
            let attrs_list = AttrsList::new(attrs);
            let mut text = [0u8; 4];
            let s: &str = ch.encode_utf8(&mut text);
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

            let (pixels, is_color) = match image.content {
                Content::Mask => {
                    let mut out = Vec::with_capacity(image.data.len() * 4);
                    for &a in &image.data {
                        out.extend_from_slice(&[a, a, a, a]);
                    }
                    (out, false)
                }
                Content::SubpixelMask => {
                    debug_assert_eq!(image.data.len() % 4, 0);
                    let mut out = Vec::with_capacity(image.data.len());
                    for px in image.data.chunks_exact(4) {
                        let r = px[0] as u32;
                        let g = px[1] as u32;
                        let b = px[2] as u32;
                        let l = ((r * 77 + g * 150 + b * 29) >> 8) as u8;
                        out.extend_from_slice(&[l, l, l, l]);
                    }
                    (out, false)
                }
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
                font_id,
            })
        })
    }

    fn measure_cell(&mut self, logical_size_pt: f32, scale: f32) -> Option<CellMetrics> {
        let font_size_px = (logical_size_pt * scale).max(1.0);
        with_font_system(|fs| {
            let family = crate::renderer::font_system::terminal_family();
            let attrs = Attrs::new().family(Family::Name(&family));
            let attrs_list = AttrsList::new(attrs);
            let mut line = BufferLine::new("M", LineEnding::None, attrs_list, Shaping::Advanced);
            let layout = line.layout(fs, font_size_px, None, Wrap::None, None, 8);
            let lg = layout.first()?.glyphs.first()?;
            let font_id = lg.font_id;
            let advance_px = lg.w;

            let font = fs.get_font(font_id)?;
            let font_ref = font.as_swash();
            let metrics = font_ref.metrics(&[]);

            let upem = metrics.units_per_em as f32;
            if upem <= 0.0 {
                return None;
            }
            let scale = font_size_px / upem;
            let ascent_px = (metrics.ascent * scale).ceil().max(1.0) as u32;
            let descent_px = (metrics.descent.abs() * scale).ceil() as u32;
            let leading_px = (metrics.leading.max(0.0) * scale).ceil() as u32;
            let cell_height = (ascent_px + descent_px + leading_px).max(1);
            let cell_width = (advance_px.ceil() as u32).max(1);

            Some(CellMetrics {
                cell_width,
                cell_height,
                ascent: ascent_px,
            })
        })
    }
}

// ── Platform dispatch ─────────────────────────────────────────────────

#[cfg(any(target_os = "ios", target_os = "macos"))]
pub(crate) type PlatformRasterizer = crate::renderer::coretext_raster::CoreTextRasterizer;

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
pub(crate) type PlatformRasterizer = SwashRasterizer;

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterizes_ascii() {
        let mut r = SwashRasterizer::new();
        let g = r
            .rasterize('A', 12.0, 2.0, false, false)
            .expect("ASCII 'A' must rasterise via Menlo");
        assert!(!g.pixels.is_empty(), "no pixels rendered for 'A'");
        assert_eq!(
            g.pixels.len() as u32,
            g.width * g.height * 4,
            "buffer size doesn't match width*height*4"
        );
        assert!(!g.is_color, "'A' came back flagged as color glyph");
    }

    #[test]
    fn rasterizes_cjk_via_font_fallback() {
        let mut r = SwashRasterizer::new();
        let ascii = r
            .rasterize('A', 12.0, 2.0, false, false)
            .expect("'A' should rasterize");
        let cjk = r
            .rasterize('中', 12.0, 2.0, false, false)
            .expect("'中' should rasterize via fallback");
        assert!(cjk.width > 0 && cjk.height > 0);
        assert!(cjk.pixels.iter().any(|&b| b != 0));
        assert_ne!(
            ascii.font_id, cjk.font_id,
            "CJK rasterised from the same font as ASCII — fallback cascade not engaged. \
             Menlo does not contain CJK; if these match the result is a tofu .notdef."
        );
    }

    #[test]
    fn rasterizes_emoji_as_color() {
        let mut r = SwashRasterizer::new();
        let g = r
            .rasterize('😀', 12.0, 2.0, false, false)
            .expect("emoji should rasterize");
        assert!(g.is_color, "emoji should rasterize as a color glyph");
        assert!(g.width >= 16 && g.height >= 16);
        let any_chromatic_opaque = g.pixels.chunks_exact(4).any(|px| {
            let (b, g_, r, a) = (px[0], px[1], px[2], px[3]);
            a == 255 && !(b == g_ && g_ == r)
        });
        assert!(
            any_chromatic_opaque,
            "no chromatic opaque pixel — emoji may have rendered as a mask"
        );
    }

    #[test]
    fn measure_cell_metrics_are_sane() {
        let mut r = SwashRasterizer::new();
        let m = r
            .measure_cell(12.0, 2.0)
            .expect("Menlo @ 24px should measure");
        assert!(m.cell_width > 0);
        assert!(m.cell_height > 0);
        assert!(m.ascent > 0);
        assert!(m.cell_width >= 8 && m.cell_width <= 32);
        assert!(m.cell_height > m.ascent);
    }

    #[test]
    fn measure_cell_scales_with_size() {
        let mut r = SwashRasterizer::new();
        let small = r.measure_cell(6.0, 2.0).expect("12px metrics");
        let large = r.measure_cell(24.0, 2.0).expect("48px metrics");
        assert!(large.cell_width >= small.cell_width * 3);
        assert!(large.cell_height >= small.cell_height * 3);
    }
}
