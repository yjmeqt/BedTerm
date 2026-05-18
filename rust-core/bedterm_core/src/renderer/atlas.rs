//! Glyph atlas — rasterise codepoints via cosmic-text/swash into a BGRA8
//! MTLTexture.
//!
//! Layout: a single 2048×2048 BGRA8Unorm texture. Each glyph occupies a quad
//! of `cell_px` (narrow) or `2 * cell_px.0` (wide / CJK / emoji) wide. The
//! row-stride bin-packer marches left-to-right at row height `cell_px.1`,
//! wraps when full, fails safe (no eviction) when the atlas is exhausted.
//!
//! Pixel format: BGRA8Unorm premultiplied. Monochrome glyphs from swash's
//! `Content::Mask` are expanded to (a,a,a,a) so the shader can tint by
//! foreground. Color glyphs (Apple Color Emoji via sbix, COLR/CPAL OpenType
//! color fonts) are stored as full RGBA (premultiplied, byte-swapped to BGRA)
//! and composited untinted.
//!
//! Font fallback: handled by cosmic-text's `FontSystem` — when the primary
//! face (Menlo) lacks a codepoint, cosmic-text walks the `fontdb` cascade and
//! the rasterizer transparently switches to PingFang / Hiragino / Apple
//! Color Emoji as needed.

use std::collections::HashMap;

use metal::{Device, MTLPixelFormat, MTLRegion, MTLTextureUsage, Texture, TextureDescriptor};

use crate::renderer::glyph_raster::{measure_cell, rasterize, CellMetrics};

const ATLAS_PX: u32 = 2048;

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub uv_origin: (f32, f32),
    pub uv_size: (f32, f32),
    pub pixel_size: (u32, u32),
    /// True if this glyph occupies two columns (CJK wide character / emoji).
    pub wide: bool,
    /// True if this glyph is a color bitmap glyph (Apple Color Emoji, COLR).
    /// The shader composites color glyphs over the cell background instead
    /// of tinting an alpha mask by the foreground colour.
    pub is_color: bool,
}

pub struct GlyphAtlas {
    pub texture: Texture,
    /// Single-column cell metrics. Wide glyphs occupy `2 * cell_px.0` width.
    pub cell_px: (u32, u32),
    pub ascent_px: u32,
    glyphs: HashMap<u32, GlyphInfo>,
    cursor_x: u32,
    cursor_y: u32,
    font_size_px: f32,
}

impl GlyphAtlas {
    pub fn new(device: &Device, pixel_size: f32, dpr: f32) -> Self {
        let scaled = (pixel_size.max(1.0) * dpr.max(1.0)).max(1.0);
        // Defensive fallback: fontdb scan failed (malformed iOS install,
        // sandboxed font directory). Coarse approximations so the atlas
        // still builds and the renderer draws *something* until the
        // FontSystem is recoverable.
        let metrics = measure_cell(scaled).unwrap_or(CellMetrics {
            cell_width: (scaled * 0.6) as u32,
            cell_height: scaled as u32,
            ascent: (scaled * 0.8) as u32,
        });

        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        desc.set_width(ATLAS_PX as u64);
        desc.set_height(ATLAS_PX as u64);
        desc.set_usage(MTLTextureUsage::ShaderRead);
        let texture = device.new_texture(&desc);

        let mut me = Self {
            texture,
            cell_px: (metrics.cell_width.max(1), metrics.cell_height.max(1)),
            ascent_px: metrics.ascent,
            glyphs: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            font_size_px: scaled,
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

    fn rasterize_and_upload(&mut self, codepoint: u32, wide: bool) -> Option<GlyphInfo> {
        let ch = char::from_u32(codepoint)?;
        let raster = rasterize(ch, self.font_size_px)?;

        // Color emoji is conventionally wide in terminals; auto-promote so
        // the bitmap isn't squished into a single-cell slot.
        let wide = wide || raster.is_color;
        let (cell_w, cell_h) = self.cell_px;
        let slot_w = if wide { cell_w * 2 } else { cell_w };
        let slot_h = cell_h;

        // Wrap to next row when this slot won't fit; fail safe when the
        // atlas is exhausted.
        if self.cursor_x + slot_w > ATLAS_PX {
            self.cursor_x = 0;
            self.cursor_y += slot_h;
        }
        if self.cursor_y + slot_h > ATLAS_PX {
            return None;
        }
        let dst_x = self.cursor_x;
        let dst_y = self.cursor_y;
        self.cursor_x += slot_w;

        // Compose the swash raster into a slot-sized BGRA buffer so the
        // upload is one MTLRegion::replace_region call.
        let bytes_per_row = (slot_w as usize) * 4;
        let mut buf = vec![0u8; bytes_per_row * (slot_h as usize)];

        // Center horizontally within the slot. Baseline-align vertically:
        // swash's `top` is the offset from baseline to glyph top, so the
        // glyph's top row in atlas space sits at `ascent_px - raster.top`.
        let x_offset = (((slot_w as i32) - (raster.width as i32)) / 2).max(0) as usize;
        let y_offset = ((self.ascent_px as i32) - raster.top).max(0) as usize;

        // Clip if the glyph overflows the slot — never panic on a malformed
        // or oversized raster.
        let copy_w = (raster.width as usize).min((slot_w as usize).saturating_sub(x_offset));
        let copy_h = (raster.height as usize).min((slot_h as usize).saturating_sub(y_offset));
        let src_stride = (raster.width as usize) * 4;

        for row in 0..copy_h {
            let src_start = row * src_stride;
            let src_end = src_start + copy_w * 4;
            let dst_row = y_offset + row;
            let dst_start = dst_row * bytes_per_row + x_offset * 4;
            let dst_end = dst_start + copy_w * 4;
            if dst_end <= buf.len() && src_end <= raster.pixels.len() {
                buf[dst_start..dst_end].copy_from_slice(&raster.pixels[src_start..src_end]);
            }
        }

        let region = MTLRegion::new_2d(dst_x as u64, dst_y as u64, slot_w as u64, slot_h as u64);
        self.texture
            .replace_region(region, 0, buf.as_ptr() as *const _, bytes_per_row as u64);

        let atlas_pxf = ATLAS_PX as f32;
        Some(GlyphInfo {
            uv_origin: (dst_x as f32 / atlas_pxf, dst_y as f32 / atlas_pxf),
            uv_size: (slot_w as f32 / atlas_pxf, slot_h as f32 / atlas_pxf),
            pixel_size: (slot_w, slot_h),
            wide,
            is_color: raster.is_color,
        })
    }
}
