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
use crate::renderer::icon_atlas::{decode as decode_icon, IconSlot};
use crate::renderer::ui_text::rasterize_ui;

const ATLAS_PX: u32 = 2048;
/// Y-pixel at which the UI / icon section of the atlas begins. Terminal
/// cells pack starting from y=0; UI glyphs + icons pack from this row
/// down. With a 2048-tall atlas, terminal cells get the top 1024 rows
/// (≈ 50 rows × 20 px tall = thousands of glyphs) and UI/icon content
/// gets the bottom 1024.
const UI_SECTION_Y: u32 = 1024;

/// Key for the proportional-font glyph cache. `font_px_hundredths` is
/// the font size in 0.01 px units so floats round-trip through `Hash`.
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub struct UIGlyphKey {
    /// First codepoint of the grapheme. For single-codepoint glyphs this
    /// is everything; multi-codepoint clusters (combining marks, ZWJ
    /// sequences) get keyed by their lead codepoint, accepting a rare
    /// cache miss on perfect re-look-up. M3 covers ASCII + common CJK;
    /// future work can extend to grapheme strings if needed.
    pub codepoint: u32,
    pub font_px_hundredths: u32,
}

/// One UI glyph or icon's location + offsets inside the shared atlas
/// texture. Mirrors the bits of `GlyphInfo` we need plus baseline /
/// origin offsets (terminal glyphs share a single baseline so they get
/// away without these fields).
#[derive(Clone, Copy, Debug)]
pub struct UISlot {
    pub uv_origin: (f32, f32),
    pub uv_size: (f32, f32),
    pub pixel_size: (u32, u32),
    /// Horizontal offset from pen position to slot's left edge (px).
    pub left: i32,
    /// Vertical offset from pen baseline to slot's top edge (px).
    /// Positive means the slot sits above the baseline.
    pub top: i32,
}

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

/// Cell-glyph cache key. Includes weight / slant so bold and italic
/// variants of the same codepoint each get their own atlas slot
/// instead of colliding on a single rasterization.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GlyphKey {
    pub codepoint: u32,
    pub bold: bool,
    pub italic: bool,
}

impl GlyphKey {
    pub fn regular(codepoint: u32) -> Self {
        Self {
            codepoint,
            bold: false,
            italic: false,
        }
    }
}

pub struct GlyphAtlas {
    pub texture: Texture,
    /// Single-column cell metrics. Wide glyphs occupy `2 * cell_px.0` width.
    pub cell_px: (u32, u32),
    pub ascent_px: u32,
    glyphs: HashMap<GlyphKey, GlyphInfo>,
    cursor_x: u32,
    cursor_y: u32,
    font_size_px: f32,
    /// UI glyph cache. Keyed by (codepoint, font_px) so the same
    /// codepoint at subheadline vs caption2 size each get their own
    /// slot.
    ui_glyphs: HashMap<UIGlyphKey, UISlot>,
    /// Icon (agent badge) cache. Keyed by slot enum so re-lookups are
    /// O(1) without re-decoding the PNG.
    icons: HashMap<IconSlot, UISlot>,
    /// Bin-packing cursor for the UI section. Increments per-row; row
    /// height is the tallest glyph placed in that row.
    ui_cursor_x: u32,
    ui_cursor_y: u32,
    ui_row_height: u32,
    /// UV pointing into a reserved 4×4 fully-opaque white texel at the
    /// top-left of the atlas. Underline / strikethrough quads sample
    /// here with `fg = cell.fg_rgba`; the cell shader's
    /// `mix(bg, fg, sample.a)` yields `fg` since `sample.a == 1`. No
    /// extra pipeline, no extra texture binding.
    pub solid_uv: (f32, f32),
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

        // Reserve a 4×4 solid-white block in the **bottom-right corner**
        // of the atlas, used as the alpha-mask source for flat
        // decorations (underline, strikethrough). It HAS to live away
        // from (0,0): empty cells (whose glyph lookup misses) emit
        // quads with UV=(0,0)..(0,0), and that texel must stay
        // transparent so the shader's `mix(bg, fg, 0) = bg` preserves
        // the cell's background. Putting solid at (0,0) makes every
        // empty cell render as `fg`, which manifests as full-cell
        // white-on-fg blocks across the viewport (regression caught
        // during SGR 4 verification).
        const SOLID_PX: u32 = 4;
        let solid_x = ATLAS_PX - SOLID_PX;
        let solid_y = ATLAS_PX - SOLID_PX;
        {
            let buf = vec![0xFFu8; (SOLID_PX * SOLID_PX * 4) as usize];
            let region = MTLRegion::new_2d(
                solid_x as u64,
                solid_y as u64,
                SOLID_PX as u64,
                SOLID_PX as u64,
            );
            texture.replace_region(region, 0, buf.as_ptr() as *const _, (SOLID_PX * 4) as u64);
        }
        let atlas_pxf = ATLAS_PX as f32;
        let solid_uv = (
            (solid_x as f32 + SOLID_PX as f32 * 0.5) / atlas_pxf,
            (solid_y as f32 + SOLID_PX as f32 * 0.5) / atlas_pxf,
        );

        let mut me = Self {
            texture,
            cell_px: (metrics.cell_width.max(1), metrics.cell_height.max(1)),
            ascent_px: metrics.ascent,
            glyphs: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            font_size_px: scaled,
            ui_glyphs: HashMap::new(),
            icons: HashMap::new(),
            ui_cursor_x: 0,
            ui_cursor_y: UI_SECTION_Y,
            ui_row_height: 0,
            solid_uv,
        };
        me.preload_ascii();
        me
    }

    /// Drop the UI / icon caches so M7's Dynamic Type re-rasterization
    /// path can repopulate at the new font sizes without leaking the
    /// old slots. The texture itself is intentionally kept — over-paint
    /// is fine, the new slots just claim fresh atlas pixels.
    pub fn reset_ui_caches(&mut self) {
        self.ui_glyphs.clear();
        self.icons.clear();
        self.ui_cursor_x = 0;
        self.ui_cursor_y = UI_SECTION_Y;
        self.ui_row_height = 0;
    }

    fn preload_ascii(&mut self) {
        for ch in 0x20u32..0x7Fu32 {
            self.ensure(GlyphKey::regular(ch), false);
        }
    }

    /// Rasterise+upload the glyph for `key` if not already in the atlas.
    /// Idempotent. `wide=true` reserves a 2-cell-wide slot for CJK
    /// double-width glyphs.
    pub fn ensure(&mut self, key: GlyphKey, wide: bool) {
        if self.glyphs.contains_key(&key) {
            return;
        }
        if let Some(info) = self.rasterize_and_upload(key, wide) {
            self.glyphs.insert(key, info);
        }
    }

    pub fn lookup(&self, key: GlyphKey) -> Option<&GlyphInfo> {
        self.glyphs.get(&key)
    }

    fn rasterize_and_upload(&mut self, key: GlyphKey, wide: bool) -> Option<GlyphInfo> {
        let ch = char::from_u32(key.codepoint)?;
        let raster = rasterize(ch, self.font_size_px, key.bold, key.italic)?;

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

    /// Ensure a UI-font glyph for `text` at `font_size_px` is present
    /// in the atlas. Returns its slot or `None` if rasterization failed
    /// (whitespace / no inked pixels) or the atlas section is full.
    pub fn ensure_ui(&mut self, text: &str, font_size_px: f32) -> Option<UISlot> {
        // Key on the lead codepoint — sufficient for ASCII / single-CJK
        // grapheme clusters which is everything header text exercises.
        let lead = text.chars().next()?;
        let key = UIGlyphKey {
            codepoint: lead as u32,
            font_px_hundredths: (font_size_px * 100.0) as u32,
        };
        if let Some(slot) = self.ui_glyphs.get(&key) {
            return Some(*slot);
        }
        let raster = rasterize_ui(text, font_size_px)?;
        let slot = self.pack_ui_bitmap(
            &raster.pixels,
            raster.width,
            raster.height,
            raster.left,
            raster.top,
        )?;
        self.ui_glyphs.insert(key, slot);
        Some(slot)
    }

    /// Ensure an icon slot is present in the atlas. Decodes the embedded
    /// PNG on first use and uploads it at native pixel resolution; the
    /// caller scales it via quad geometry at draw time.
    pub fn ensure_icon(&mut self, slot_id: IconSlot) -> Option<UISlot> {
        if let Some(slot) = self.icons.get(&slot_id) {
            return Some(*slot);
        }
        let decoded = decode_icon(slot_id);
        // Icons treat their alpha channel as a coverage mask (see
        // `icon_atlas::decode`) — they emit (a, a, a, a) bytes and the
        // cell shader tints them via `fg`. `left`/`top` are zero since
        // icons render at the quad's exact rect, not relative to a
        // text baseline.
        let slot = self.pack_ui_bitmap(&decoded.bgra, decoded.width, decoded.height, 0, 0)?;
        self.icons.insert(slot_id, slot);
        Some(slot)
    }

    /// Bin-pack a raster bitmap into the UI section. Returns `None` if
    /// the bitmap is empty or the section is full.
    fn pack_ui_bitmap(
        &mut self,
        pixels: &[u8],
        w: u32,
        h: u32,
        left: i32,
        top: i32,
    ) -> Option<UISlot> {
        if w == 0 || h == 0 {
            return None;
        }
        // Wrap to the next row when this glyph won't fit horizontally.
        if self.ui_cursor_x + w > ATLAS_PX {
            self.ui_cursor_y += self.ui_row_height.max(1);
            self.ui_cursor_x = 0;
            self.ui_row_height = 0;
        }
        // Fail safe when the section is exhausted — caller skips the
        // glyph rather than panicking.
        if self.ui_cursor_y + h > ATLAS_PX {
            return None;
        }
        let dst_x = self.ui_cursor_x;
        let dst_y = self.ui_cursor_y;
        self.ui_cursor_x += w;
        self.ui_row_height = self.ui_row_height.max(h);

        let bytes_per_row = (w as usize) * 4;
        let region = MTLRegion::new_2d(dst_x as u64, dst_y as u64, w as u64, h as u64);
        self.texture
            .replace_region(region, 0, pixels.as_ptr() as *const _, bytes_per_row as u64);

        let atlas_pxf = ATLAS_PX as f32;
        Some(UISlot {
            uv_origin: (dst_x as f32 / atlas_pxf, dst_y as f32 / atlas_pxf),
            uv_size: (w as f32 / atlas_pxf, h as f32 / atlas_pxf),
            pixel_size: (w, h),
            left,
            top,
        })
    }
}
