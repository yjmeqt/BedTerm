//! CoreText + CoreGraphics glyph rasterizer for Apple platforms.
//!
//! Replaces the cosmic-text + swash pipeline on iOS / macOS so the
//! renderer benefits from CoreText's system font cascade — characters
//! that Menlo doesn't cover (Misc Technical, dingbats, etc.) are
//! automatically resolved to the system font that has them.
//!
//! ## Known issues (2026-05-24)
//!
//! - **U+2588 █ Full Block**: CoreText's tight bounding box via
//!   `get_bounding_rects_for_glyphs` may be 1–2 px narrower than the
//!   glyph's advance, causing horizontal gaps when full-block characters
//!   are tiled adjacently. Swash reports placement at the advance width,
//!   so it doesn't have this issue. Fix: use `max(bbox_width, advance)`
//!   for the rasterized bitmap width so cell-filling glyphs span the
//!   full slot.
//! - **U+23FA ⏺ Black Circle for Record**: rendered via
//!   `CTFontCreateForString` fallback. The system fallback font's metrics
//!   (ascent/descent ratio, advance) differ from Menlo's cell grid,
//!   causing misalignment within the monospace cell. Fix: snap fallback
//!   glyph dimensions to the host cell metrics before atlas upload, or
//!   rasterize into a cell-sized slot directly.

#![cfg(any(target_os = "ios", target_os = "macos"))]

use core_foundation::base::{CFRange, TCFType, TCFTypeRef};
use core_foundation::string::{CFString, UniChar};
use core_graphics::base::{kCGBitmapByteOrder32Little, kCGImageAlphaPremultipliedFirst, CGFloat};
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::{CGContext, CGTextDrawingMode};
use core_graphics::geometry::CGPoint;
use core_text::font::CTFont;
use core_text::font_descriptor::{
    kCTFontBoldTrait, kCTFontItalicTrait, kCTFontOrientationDefault, CTFontSymbolicTraits,
};
use std::collections::HashMap;

use super::font_system::MENLO_BYTES;
use crate::renderer::glyph_raster::{CellMetrics, GlyphRasterizer, RasterizedGlyph};

// CTFontCreateForString is not yet exposed by the core-text crate.
// We bind it manually via raw pointers for FFI safety, then wrap
// with safe Rust types at the call site.
use core_foundation::string::CFStringRef;
use std::os::raw::c_void;
extern "C" {
    fn CTFontCreateForString(
        current_font: *const c_void,
        string: CFStringRef,
        range: CFRange,
    ) -> *const c_void;
}

pub struct CoreTextRasterizer {
    /// Primary Menlo CTFont at the current logical size (regular weight).
    primary_ct_font: Option<CTFont>,
    /// Bold / italic variants of the primary font, keyed by
    /// (logical_size_hundredths, bold, italic).
    variant_cache: HashMap<(u32, bool, bool), CTFont>,
    /// System fallback fonts for codepoints Menlo doesn't cover.
    fallback_cache: HashMap<char, CTFont>,
    /// Cached cell metrics for the current size.
    cached_metrics: Option<CellMetrics>,
    /// Logical size at which cached metrics / primary font were computed.
    cached_pt: f32,
    cached_scale: f32,
}

impl CoreTextRasterizer {
    pub fn new() -> Self {
        Self {
            primary_ct_font: None,
            variant_cache: HashMap::new(),
            fallback_cache: HashMap::new(),
            cached_metrics: None,
            cached_pt: 0.0,
            cached_scale: 0.0,
        }
    }

    /// Get or create the primary CTFont at `logical_size_pt`.
    fn ensure_primary(&mut self, logical_size_pt: f32) -> Option<&CTFont> {
        if self.cached_pt != logical_size_pt || self.primary_ct_font.is_none() {
            let font = if let Some(bytes) = MENLO_BYTES.get() {
                core_text::font::new_from_buffer(bytes).ok()?
            } else {
                // Fall back to system font lookup (macOS test / dev).
                core_text::font::new_from_name("Menlo", logical_size_pt as CGFloat).ok()?
            };
            self.primary_ct_font = Some(font.clone_with_font_size(logical_size_pt as CGFloat));
            self.cached_pt = logical_size_pt;
            self.variant_cache.clear();
            self.cached_metrics = None;
        }
        self.primary_ct_font.as_ref()
    }

    /// Get a CTFont variant (bold, italic, or both) at the given size.
    fn ensure_variant(&mut self, logical_size_pt: f32, bold: bool, italic: bool) -> Option<CTFont> {
        let key = ((logical_size_pt * 100.0) as u32, bold, italic);
        if let Some(cached) = self.variant_cache.get(&key) {
            return Some(cached.clone());
        }

        let base = self.ensure_primary(logical_size_pt)?.clone();

        let mut traits: CTFontSymbolicTraits = 0;
        if bold {
            traits |= kCTFontBoldTrait;
        }
        if italic {
            traits |= kCTFontItalicTrait;
        }

        let variant = if traits == 0 {
            base
        } else {
            base.clone_with_symbolic_traits(traits, traits)
                .unwrap_or(base)
        };

        self.variant_cache.insert(key, variant.clone());
        Some(variant)
    }

    /// Find a system fallback font that covers `ch`.
    fn fallback_for_char(
        &mut self,
        ch: char,
        logical_size_pt: f32,
        current_font: &CTFont,
    ) -> Option<CTFont> {
        if let Some(cached) = self.fallback_cache.get(&ch) {
            return Some(cached.clone());
        }

        let mut buf = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut buf);
        let cf_string = CFString::new(s);
        let range = CFRange {
            location: 0,
            length: cf_string.char_len(),
        };

        let raw_font = unsafe {
            CTFontCreateForString(
                current_font.as_concrete_TypeRef().as_void_ptr(),
                cf_string.as_concrete_TypeRef().as_void_ptr() as CFStringRef,
                range,
            )
        };
        if raw_font.is_null() {
            return None;
        }
        // Convert the raw CTFont pointer back into a Rust CTFont.
        // Safety: CTFontCreateForString returns a +1-retained CTFont.
        let fallback = unsafe { CTFont::wrap_under_create_rule(raw_font as _) };

        let sized = fallback.clone_with_font_size(logical_size_pt as CGFloat);
        self.fallback_cache.insert(ch, sized.clone());
        Some(sized)
    }
}

impl GlyphRasterizer for CoreTextRasterizer {
    fn rasterize(
        &mut self,
        ch: char,
        logical_size_pt: f32,
        scale: f32,
        bold: bool,
        italic: bool,
    ) -> Option<RasterizedGlyph> {
        let ct_font = self.ensure_variant(logical_size_pt, bold, italic)?;

        // Encode char as UTF-16 (UniChar = u16).
        let mut utf16_buf = [0u16; 2];
        let utf16_len = ch.encode_utf16(&mut utf16_buf).len();
        let chars_ptr: *const UniChar = utf16_buf.as_ptr();

        // Get glyph ID from primary font.
        let mut glyphs = [0u16; 2];
        unsafe {
            ct_font.get_glyphs_for_characters(chars_ptr, glyphs.as_mut_ptr(), utf16_len as isize);
        }

        let (render_font, render_glyph) = if glyphs[0] == 0 {
            let fb = self.fallback_for_char(ch, logical_size_pt, &ct_font)?;
            let mut fb_glyphs = [0u16; 2];
            unsafe {
                fb.get_glyphs_for_characters(chars_ptr, fb_glyphs.as_mut_ptr(), utf16_len as isize);
            }
            if fb_glyphs[0] == 0 {
                return None;
            }
            (fb, fb_glyphs[0])
        } else {
            (ct_font, glyphs[0])
        };

        // Bounding box in points.
        let bbox =
            render_font.get_bounding_rects_for_glyphs(kCTFontOrientationDefault, &[render_glyph]);

        let bbox_w = bbox.size.width as f32;
        let bbox_h = bbox.size.height as f32;
        if bbox_w <= 0.0 || bbox_h <= 0.0 {
            return None;
        }

        let pixel_w = (bbox_w * scale).ceil().max(1.0) as usize;
        let pixel_h = (bbox_h * scale).ceil().max(1.0) as usize;

        let color_space = CGColorSpace::create_device_rgb();
        let bitmap_info = kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little;
        let mut ctx = CGContext::create_bitmap_context(
            None,
            pixel_w,
            pixel_h,
            8,
            pixel_w * 4,
            &color_space,
            bitmap_info,
        );

        // Grayscale antialiasing only — no subpixel fringing.
        ctx.set_should_antialias(true);
        ctx.set_allows_font_smoothing(false);

        // Fill white so the glyph alpha becomes coverage.
        ctx.set_rgb_fill_color(1.0, 1.0, 1.0, 1.0);
        ctx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);

        // Transform: scale to device pixels, offset by bbox origin.
        let tx = -bbox.origin.x as CGFloat * scale as CGFloat;
        let ty = -bbox.origin.y as CGFloat * scale as CGFloat;
        ctx.translate(tx, ty);
        ctx.scale(scale as CGFloat, scale as CGFloat);

        let pos = CGPoint::new(0.0, 0.0);
        render_font.draw_glyphs(&[render_glyph], &[pos], ctx.clone());

        // Read back. CGContext::data() returns &mut [u8] directly.
        let pixel_data = ctx.data().to_vec();

        // Detect color glyphs: any pixel where R/G/B aren't all equal.
        let is_color = pixel_data
            .chunks_exact(4)
            .any(|px| px[3] != 0 && !(px[0] == px[1] && px[1] == px[2]));

        let top = ((bbox.origin.y + bbox.size.height) * scale as f64) as f32; // glyph top above baseline
        let left = (bbox.origin.x * scale as f64) as f32;

        Some(RasterizedGlyph {
            pixels: pixel_data,
            width: pixel_w as u32,
            height: pixel_h as u32,
            left: left as i32,
            top: top.ceil() as i32,
            is_color,
            font_id: cosmic_text::fontdb::ID::default(),
        })
    }

    fn measure_cell(&mut self, logical_size_pt: f32, scale: f32) -> Option<CellMetrics> {
        if self.cached_metrics.is_some()
            && (self.cached_pt - logical_size_pt).abs() < 0.1
            && (self.cached_scale - scale).abs() < 0.01
        {
            return self.cached_metrics;
        }

        let ct_font = self.ensure_primary(logical_size_pt)?;

        let ascent_pt = ct_font.ascent() as f32;
        let descent_pt = ct_font.descent() as f32;
        let leading_pt = ct_font.leading() as f32;

        // Advance of 'M'.
        let mut utf16_buf = [0u16; 2];
        let utf16_len = 'M'.encode_utf16(&mut utf16_buf).len();
        let mut glyph = [0u16; 2];
        unsafe {
            ct_font.get_glyphs_for_characters(
                utf16_buf.as_ptr(),
                glyph.as_mut_ptr(),
                utf16_len as isize,
            );
        }
        if glyph[0] == 0 {
            return None;
        }

        let advances = unsafe {
            ct_font.get_advances_for_glyphs(
                kCTFontOrientationDefault,
                glyph.as_ptr(),
                std::ptr::null_mut(),
                1,
            )
        };
        let advance_pt = advances as f32;

        let cell_width = (advance_pt * scale).ceil().max(1.0) as u32;
        let ascent_px = (ascent_pt * scale).ceil().max(1.0) as u32;
        let descent_px = (descent_pt.abs() * scale).ceil() as u32;
        let leading_px = (leading_pt.max(0.0) * scale).ceil() as u32;
        let cell_height = (ascent_px + descent_px + leading_px).max(1);

        let metrics = CellMetrics {
            cell_width,
            cell_height,
            ascent: ascent_px,
        };

        self.cached_metrics = Some(metrics);
        self.cached_scale = scale;
        Some(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::glyph_raster::GlyphRasterizer;

    #[test]
    fn coretext_rasterizes_ascii() {
        let mut rt = CoreTextRasterizer::new();
        let g = rt
            .rasterize('A', 14.0, 2.0, false, false)
            .expect("ASCII 'A' must rasterize via CoreText");
        assert!(g.width > 0 && g.height > 0);
        assert!(!g.is_color);
    }

    #[test]
    fn coretext_rasterizes_symbols_not_in_menlo() {
        let mut rt = CoreTextRasterizer::new();
        // These should exist in system symbol fonts but not Menlo.
        for ch in ['\u{23F5}', '\u{23FA}', '\u{23BF}'] {
            let g = rt
                .rasterize(ch, 14.0, 2.0, false, false)
                .unwrap_or_else(|| {
                    panic!(
                        "symbol U+{:04X} must rasterize via CoreText fallback",
                        ch as u32
                    )
                });
            assert!(
                g.width > 0 && g.height > 0,
                "symbol U+{:04X} produced zero-dimension raster",
                ch as u32
            );
        }
    }

    #[test]
    fn coretext_measure_cell_is_sane() {
        let mut rt = CoreTextRasterizer::new();
        let m = rt.measure_cell(14.0, 2.0).expect("cell metrics");
        assert!(m.cell_width > 0);
        assert!(m.cell_height > 0);
        assert!(m.ascent > 0);
        assert!(
            m.cell_width >= 8 && m.cell_width <= 40,
            "cell_width {} out of plausible range",
            m.cell_width
        );
        assert!(m.cell_height > m.ascent);
    }
}
// (appended test will be removed)
