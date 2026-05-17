//! Glyph atlas — rasterise codepoints via CoreText into an R8 MTLTexture.
//!
//! Layout: a single 2048×2048 R8Unorm texture. Each glyph occupies a fixed
//! `cell_px` quad. Bin-packing is a row-stride flat advance: glyphs march
//! left-to-right across a row, wrap when the row is full, fail safe (no
//! eviction) when the whole atlas is full.

use std::collections::HashMap;

use core_graphics::base::{kCGImageAlphaNone, CGFloat};
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::{CGContext, CGTextDrawingMode};
use core_graphics::geometry::{CGPoint, CGSize};
use core_text::font::{self as ctfont, CTFont};
use core_text::font_descriptor::kCTFontOrientationHorizontal;
use metal::{
    Device, MTLPixelFormat, MTLRegion, MTLTextureUsage, Texture, TextureDescriptor,
};

const ATLAS_PX: u32 = 2048;

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub uv_origin: (f32, f32),
    pub uv_size: (f32, f32),
    pub pixel_size: (u32, u32),
}

pub struct GlyphAtlas {
    pub texture: Texture,
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
            self.ensure(ch);
        }
    }

    /// Rasterise+upload `codepoint` if not already in the atlas. Idempotent.
    pub fn ensure(&mut self, codepoint: u32) {
        if self.glyphs.contains_key(&codepoint) {
            return;
        }
        if let Some(info) = self.rasterize_and_upload(codepoint) {
            self.glyphs.insert(codepoint, info);
        }
    }

    pub fn lookup(&self, codepoint: u32) -> Option<&GlyphInfo> {
        self.glyphs.get(&codepoint)
    }

    fn rasterize_and_upload(&mut self, codepoint: u32) -> Option<GlyphInfo> {
        let ch = char::from_u32(codepoint)?;

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
        if !ok {
            return None;
        }
        let glyph = glyphs[0];
        if glyph == 0 {
            return None;
        }

        let (w, h) = self.cell_px;

        // Wrap to next row if needed; fail safe when atlas is full.
        if self.cursor_x + w > ATLAS_PX {
            self.cursor_x = 0;
            self.cursor_y += h;
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
        let descent_px: CGFloat = self.font.descent();
        let positions = [CGPoint::new(0.0, descent_px)];
        let glyph_arr: [u16; 1] = [glyph];
        // draw_glyphs takes an owned CGContext (foreign_type with retain on clone).
        self.font.draw_glyphs(&glyph_arr, &positions, ctx.clone());

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
        })
    }
}

