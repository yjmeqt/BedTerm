//! Glyph atlas — rasterise codepoints via CoreText into an R8 MTLTexture.
//!
//! Layout: a single 2048×2048 R8Unorm texture. Each glyph occupies a quad of
//! `cell_px` (narrow) or `2 * cell_px.0` (wide / CJK) wide. Bin-packing is a
//! row-stride flat advance: glyphs march left-to-right across a row at the
//! atlas row-stride (the row height equals `cell_px.1`), wrap when the row is
//! full, fail safe (no eviction) when the whole atlas is full.
//!
//! CJK / non-ASCII handling: the primary font (Menlo) only covers Latin +
//! a handful of other scripts. When `CTFontGetGlyphsForCharacters` reports a
//! missing glyph (returns false or yields glyph id 0) we fall back to
//! `CTFontCreateForString`, which asks CoreText to substitute a font with
//! coverage for the requested codepoint (PingFang for Chinese, Hiragino for
//! Japanese, Apple SD Gothic for Korean, etc.). Wide East Asian glyphs are
//! rasterised into a `2 * cell_w` buffer so they don't get clipped to a
//! single cell, and the renderer side draws their quad spanning two cells.

use std::collections::HashMap;

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::base::{kCGImageAlphaNone, CGFloat};
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::{CGContext, CGTextDrawingMode};
use core_graphics::geometry::{CGPoint, CGSize};
use core_text::font::{self as ctfont, CTFont, CTFontRef};
use core_text::font_descriptor::kCTFontOrientationHorizontal;
use metal::{Device, MTLPixelFormat, MTLRegion, MTLTextureUsage, Texture, TextureDescriptor};

const ATLAS_PX: u32 = 2048;

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    // CTFontCreateForString returns a font with coverage for the substring
    // [range.location, range.location+range.length). It substitutes a system
    // fallback when the receiver lacks the glyphs (CJK, Emoji, etc.). Caller
    // owns the returned CTFontRef (+1 retain).
    fn CTFontCreateForString(
        currentFont: CTFontRef,
        string: CFStringRef,
        range: CFRange,
    ) -> CTFontRef;
}

#[repr(C)]
struct CFRange {
    location: isize,
    length: isize,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub uv_origin: (f32, f32),
    pub uv_size: (f32, f32),
    pub pixel_size: (u32, u32),
    /// True if this glyph occupies two columns (CJK wide character).
    pub wide: bool,
}

pub struct GlyphAtlas {
    pub texture: Texture,
    /// Single-column cell metrics. Wide glyphs occupy `2 * cell_px.0` width.
    pub cell_px: (u32, u32),
    pub ascent_px: u32,
    glyphs: HashMap<u32, GlyphInfo>,
    cursor_x: u32,
    cursor_y: u32,
    font: CTFont,
}

impl GlyphAtlas {
    pub fn new(device: &Device, pixel_size: f32, dpr: f32) -> Self {
        let scaled = (pixel_size.max(1.0) * dpr.max(1.0)) as f64;

        // Prefer Menlo; fall back to Courier, then to a UI monospace font.
        let font = ctfont::new_from_name("Menlo", scaled)
            .or_else(|_| ctfont::new_from_name("Courier", scaled))
            .unwrap_or_else(|_| {
                // kCTFontUIFontUserFixedPitch == 5 (CTFontUIFontType).
                ctfont::new_ui_font_for_language(5, scaled, None)
            });

        let ascent = font.ascent().ceil() as u32;
        let descent = font.descent().ceil() as u32;
        let leading = font.leading().ceil() as u32;

        // Measure cell width via the advance of 'M'.
        let cell_w = {
            let chars: [u16; 1] = ['M' as u16];
            let mut glyphs: [u16; 1] = [0];
            unsafe {
                font.get_glyphs_for_characters(chars.as_ptr(), glyphs.as_mut_ptr(), 1);
            }
            let mut advances: [CGSize; 1] = [CGSize::new(0.0, 0.0)];
            let adv = unsafe {
                font.get_advances_for_glyphs(
                    kCTFontOrientationHorizontal,
                    glyphs.as_ptr(),
                    advances.as_mut_ptr(),
                    1,
                )
            };
            (adv.ceil() as u32).max(1)
        };
        let cell_h = (ascent + descent + leading).max(1);

        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::R8Unorm);
        desc.set_width(ATLAS_PX as u64);
        desc.set_height(ATLAS_PX as u64);
        desc.set_usage(MTLTextureUsage::ShaderRead);
        let texture = device.new_texture(&desc);

        let mut me = Self {
            texture,
            cell_px: (cell_w, cell_h),
            ascent_px: ascent,
            glyphs: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            font,
        };
        me.preload_ascii();
        me
    }

    fn preload_ascii(&mut self) {
        for ch in 0x20u32..0x7Fu32 {
            self.ensure(ch, false);
        }
    }

    /// Rasterise+upload `codepoint` if not already in the atlas. Idempotent.
    /// `wide=true` reserves a 2-cell-wide slot for CJK double-width glyphs.
    pub fn ensure(&mut self, codepoint: u32, wide: bool) {
        if self.glyphs.contains_key(&codepoint) {
            return;
        }
        if let Some(info) = self.rasterize_and_upload(codepoint, wide) {
            self.glyphs.insert(codepoint, info);
        }
    }

    pub fn lookup(&self, codepoint: u32) -> Option<&GlyphInfo> {
        self.glyphs.get(&codepoint)
    }

    /// Pick a font that has coverage for `ch`. Returns `(font, glyph_id)`.
    /// Falls back through `CTFontCreateForString` when the primary font lacks
    /// the glyph (typical for CJK / Emoji / extended scripts).
    fn resolve_glyph(&self, ch: char) -> Option<(CTFont, u16)> {
        // UTF-16 encode the codepoint.
        let mut utf16 = [0u16; 2];
        let utf16_len = ch.encode_utf16(&mut utf16).len();

        let mut glyphs: [u16; 2] = [0, 0];
        let ok = unsafe {
            self.font.get_glyphs_for_characters(
                utf16.as_ptr(),
                glyphs.as_mut_ptr(),
                utf16_len as isize,
            )
        };
        if ok && glyphs[0] != 0 {
            return Some((self.font.clone(), glyphs[0]));
        }

        // Ask CoreText for a fallback font that covers this string. The
        // primary font's `pt_size` is preserved so the fallback rasterises
        // at the same metrics.
        let cf_str = CFString::new(&ch.to_string());
        let range = CFRange {
            location: 0,
            length: utf16_len as isize,
        };
        let fallback_ref = unsafe {
            CTFontCreateForString(
                self.font.as_concrete_TypeRef(),
                cf_str.as_concrete_TypeRef(),
                range,
            )
        };
        if fallback_ref.is_null() {
            return None;
        }
        // CTFontCreateForString returns +1 retained — wrap_under_create_rule
        // adopts ownership without an extra retain.
        let fallback = unsafe { CTFont::wrap_under_create_rule(fallback_ref) };

        let ok2 = unsafe {
            fallback.get_glyphs_for_characters(
                utf16.as_ptr(),
                glyphs.as_mut_ptr(),
                utf16_len as isize,
            )
        };
        if ok2 && glyphs[0] != 0 {
            Some((fallback, glyphs[0]))
        } else {
            None
        }
    }

    fn rasterize_and_upload(&mut self, codepoint: u32, wide: bool) -> Option<GlyphInfo> {
        let ch = char::from_u32(codepoint)?;
        let (font, glyph) = self.resolve_glyph(ch)?;

        let (cell_w, cell_h) = self.cell_px;
        // Wide glyphs (CJK) occupy two horizontal cells. Allocate a 2-cell
        // buffer so the rasterised bitmap isn't clipped at the cell boundary.
        let w = if wide { cell_w * 2 } else { cell_w };
        let h = cell_h;

        // Wrap to next row if needed; fail safe when atlas is full.
        if self.cursor_x + w > ATLAS_PX {
            self.cursor_x = 0;
            self.cursor_y += cell_h;
        }
        if self.cursor_y + h > ATLAS_PX {
            return None;
        }
        let dst_x = self.cursor_x;
        let dst_y = self.cursor_y;
        self.cursor_x += w;

        // Rasterise into an R8 buffer via CGBitmapContext (grayscale, alpha=none).
        let mut pixels: Vec<u8> = vec![0u8; (w as usize) * (h as usize)];
        let color_space = CGColorSpace::create_device_gray();
        let ctx = CGContext::create_bitmap_context(
            Some(pixels.as_mut_ptr() as *mut _),
            w as usize,
            h as usize,
            8,
            w as usize,
            &color_space,
            kCGImageAlphaNone,
        );
        ctx.set_should_antialias(true);
        ctx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
        ctx.set_gray_fill_color(1.0, 1.0); // white text on black background

        // Baseline at (0, descent). CG coordinate origin is bottom-left.
        // Use the resolved font's descent — fallback fonts may have slightly
        // different metrics than the primary, but using the primary's descent
        // keeps every glyph aligned on the same baseline within a row.
        let descent_px: CGFloat = self.font.descent();
        let positions = [CGPoint::new(0.0, descent_px)];
        let glyph_arr: [u16; 1] = [glyph];
        // draw_glyphs takes an owned CGContext (foreign_type with retain on clone).
        font.draw_glyphs(&glyph_arr, &positions, ctx.clone());

        // Drop the CG context to flush.
        drop(ctx);

        // Upload into the atlas texture sub-region.
        let region = MTLRegion::new_2d(dst_x as u64, dst_y as u64, w as u64, h as u64);
        self.texture
            .replace_region(region, 0, pixels.as_ptr() as *const _, w as u64);

        let atlas_pxf = ATLAS_PX as f32;
        Some(GlyphInfo {
            uv_origin: (dst_x as f32 / atlas_pxf, dst_y as f32 / atlas_pxf),
            uv_size: (w as f32 / atlas_pxf, h as f32 / atlas_pxf),
            pixel_size: (w, h),
            wide,
        })
    }
}
