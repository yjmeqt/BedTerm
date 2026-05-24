//! Layout + draw for per-block header bands.
//!
//! Single entry point — `emit_header` — consumed by
//! `Renderer::draw_block_list`. Builds quad vertices for:
//!   - divider hairline above the header (optional, skipped for sticky)
//!   - header background fill
//!   - badge circle + icon glyph (when `agent_id != 0`)
//!   - command text run (truncated with ellipsis)
//!   - subtitle text run (optional)
//!
//! Sticky headers go through the same draw subroutine; the caller
//! arranges draw order so the sticky band paints last (z-order), and
//! sets `header_y_top_px` in screen-space rather than content-space.
//!
//! Layout constants (paddings, badge size, row gap) live in this file
//! and mirror what `BlockHeader.swift` used to do at the SwiftUI layer.
//! Keeping the constants here means the Swift descriptor builder only
//! sources data (strings, colors, ids), not geometry.

use crate::renderer::atlas::{GlyphAtlas, GlyphInfo, GlyphKey, UISlot};
use crate::renderer::block_list_ffi::BtBlockHeaderEntry;
use crate::renderer::cells::{CellVertex, PanelVertex, VERTICES_PER_CELL, VERTICES_PER_PANEL};
use crate::renderer::icon_atlas::slot_for_agent;
use crate::renderer::ui_text::{fit_prefix, shape_advances};

/// Layout constants in **logical points** — multiplied by `ctx.scale`
/// at use sites. Matches BlockPanelStyle / BlockHeader's original
/// values so the visual layout is byte-identical to today.
mod c {
    pub const HORIZONTAL_PADDING_PT: f32 = 12.0;
    pub const BADGE_DIAMETER_PT: f32 = 28.0;
    pub const ICON_SIZE_PT: f32 = 14.0;
    pub const BADGE_TEXT_GAP_PT: f32 = 8.0;
    pub const ROW_GAP_PT: f32 = 2.0;
    pub const DIVIDER_THICKNESS_PT: f32 = 1.0;
}

pub(crate) struct HeaderDrawContext<'a> {
    pub subheadline_px: f32,
    pub caption2_px: f32,
    pub scale: f32,
    #[allow(dead_code)]
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub scroll_y_px: f32,
    pub surface_bg_rgba: u32,
    pub atlas: &'a mut GlyphAtlas,
    /// Width of a terminal cell in pixels — used for monospace command text.
    pub cell_w_px: f32,
    /// Height of a terminal cell in pixels.
    pub cell_h_px: f32,
}

/// Emit vertices for one header descriptor into the caller's panel and
/// cell vertex buffers. Off-screen headers are culled cheaply.
pub(crate) fn emit_header(
    ctx: &mut HeaderDrawContext<'_>,
    header: &BtBlockHeaderEntry,
    panel_verts: &mut Vec<PanelVertex>,
    cell_verts: &mut Vec<CellVertex>,
) {
    // Sticky descriptors arrive in screen-space; natural headers are in
    // content-space and must be adjusted by the scroll offset.
    let y_screen = if header.is_sticky != 0 {
        header.header_y_top_px
    } else {
        header.header_y_top_px - ctx.scroll_y_px
    };
    let h = header.header_height_px;

    // Cull off-screen.
    if y_screen + h <= 0.0 || y_screen >= ctx.viewport_h {
        return;
    }

    // Divider hairline.
    //   - Natural header: above the band — separates this block from
    //     the one above it in the list.
    //   - Sticky header: below the band — separates the pinned chrome
    //     from the body content scrolling underneath, so the sticky
    //     band reads as a section header floating above the list.
    if header.divider_rgba != 0 {
        let thickness = c::DIVIDER_THICKNESS_PT * ctx.scale;
        let divider_y = if header.is_sticky != 0 {
            y_screen + h
        } else {
            y_screen - thickness
        };
        append_panel(
            panel_verts,
            header.panel_x_left_px,
            divider_y,
            header.panel_width_px,
            thickness,
            0.0,
            header.divider_rgba,
        );
    }

    // Header band fill — explicitly paint with the body colour
    // (terminal palette `default_bg`). Same value the renderer clears
    // to, so the band is visually flush with the cell surface, but
    // painting it as a real opaque rectangle (rather than relying on
    // a hole in the geometry) means sticky bands occlude scrolling
    // body cells underneath, and natural bands stay a tangible surface
    // — not transparent chrome.
    append_panel(
        panel_verts,
        header.panel_x_left_px,
        y_screen,
        header.panel_width_px,
        h,
        0.0,
        ctx.surface_bg_rgba,
    );

    // Badge (circle + icon).
    let pad_left = c::HORIZONTAL_PADDING_PT * ctx.scale;
    let mut text_cursor_x = header.panel_x_left_px + pad_left;
    if let Some(icon_slot_id) = slot_for_agent(header.agent_id) {
        let badge_diameter = c::BADGE_DIAMETER_PT * ctx.scale;
        let icon_size = c::ICON_SIZE_PT * ctx.scale;
        let badge_x = header.panel_x_left_px + pad_left;
        let badge_y = y_screen + (h - badge_diameter) / 2.0;

        // Brand-tint circle (corner_radius = half side -> circle).
        append_panel(
            panel_verts,
            badge_x,
            badge_y,
            badge_diameter,
            badge_diameter,
            badge_diameter / 2.0,
            header.badge_tint_rgba,
        );

        // Icon glyph centered inside the circle, rendered through the
        // cell pipeline (alpha-mask path) with bg = badge tint and
        // fg = white, so the icon's transparent regions blend to the
        // tint and its inked regions fade to white.
        if let Some(slot) = ctx.atlas.ensure_icon(icon_slot_id) {
            let icon_x = badge_x + (badge_diameter - icon_size) / 2.0;
            let icon_y = badge_y + (badge_diameter - icon_size) / 2.0;
            append_textured_quad(
                cell_verts,
                icon_x,
                icon_y,
                icon_size,
                icon_size,
                &slot,
                /* fg */ 0xFFFF_FFFF,
                /* bg */ header.badge_tint_rgba,
            );
        }
        text_cursor_x += badge_diameter + c::BADGE_TEXT_GAP_PT * ctx.scale;
    }

    // Text runs.
    let right_pad = c::HORIZONTAL_PADDING_PT * ctx.scale;
    let max_text_w =
        (header.panel_x_left_px + header.panel_width_px - right_pad - text_cursor_x).max(0.0);

    let sub_px = ctx.subheadline_px.max(1.0);
    let cap_px = ctx.caption2_px.max(1.0);
    let row_gap = c::ROW_GAP_PT * ctx.scale;

    let has_subtitle = header.subtitle_len > 0;
    let stack_h = if has_subtitle {
        sub_px + row_gap + cap_px
    } else {
        sub_px
    };
    let stack_top = y_screen + (h - stack_h) / 2.0;

    if let Some(cmd) = header_str(header.command_utf8, header.command_len) {
        // Center cell-height text vertically in the header band.
        let cell_baseline = y_screen + (h + ctx.cell_h_px) * 0.5;
        emit_mono_text_run(
            ctx,
            cmd,
            text_cursor_x,
            cell_baseline,
            max_text_w,
            header.command_fg_rgba,
            ctx.surface_bg_rgba,
            cell_verts,
        );
    }
    if has_subtitle {
        if let Some(sub) = header_str(header.subtitle_utf8, header.subtitle_len) {
            let baseline = stack_top + sub_px + row_gap + cap_px * 0.78;
            emit_text_run(
                ctx,
                sub,
                cap_px,
                text_cursor_x,
                baseline,
                max_text_w,
                header.subtitle_fg_rgba,
                ctx.surface_bg_rgba,
                cell_verts,
            );
        }
    }
}

/// SAFETY: caller (Swift) guarantees `ptr..ptr+len` is valid UTF-8 for
/// the duration of the FFI call. Returns `None` for null pointers or
/// invalid UTF-8 (defensive — Swift's `String.data(using: .utf8)` always
/// emits valid bytes).
fn header_str<'a>(ptr: *const u8, len: u32) -> Option<&'a str> {
    if ptr.is_null() || len == 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

#[allow(clippy::too_many_arguments)]
fn emit_text_run(
    ctx: &mut HeaderDrawContext<'_>,
    text: &str,
    font_px: f32,
    start_x: f32,
    baseline_y: f32,
    max_w: f32,
    fg_rgba: u32,
    bg_rgba: u32,
    cell_verts: &mut Vec<CellVertex>,
) {
    if text.is_empty() || max_w <= 0.0 {
        return;
    }
    let glyphs = shape_advances(text, font_px);
    let ellipsis_glyphs = shape_advances("…", font_px);
    let ellipsis_w: f32 = ellipsis_glyphs.iter().map(|g| g.advance_px).sum();
    let (take, needs_ellipsis, _w) = fit_prefix(&glyphs, ellipsis_w, max_w);

    let mut x = start_x;
    for g in glyphs.iter().take(take) {
        emit_glyph_quad(
            ctx, &g.text, font_px, x, baseline_y, fg_rgba, bg_rgba, cell_verts,
        );
        x += g.advance_px;
    }
    if needs_ellipsis {
        for g in ellipsis_glyphs.iter() {
            emit_glyph_quad(
                ctx, &g.text, font_px, x, baseline_y, fg_rgba, bg_rgba, cell_verts,
            );
            x += g.advance_px;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_glyph_quad(
    ctx: &mut HeaderDrawContext<'_>,
    text: &str,
    font_px: f32,
    pen_x: f32,
    baseline_y: f32,
    fg_rgba: u32,
    bg_rgba: u32,
    cell_verts: &mut Vec<CellVertex>,
) {
    let Some(slot) = ctx.atlas.ensure_ui(text, font_px) else {
        return; // whitespace or atlas full — skip silently
    };
    let x = pen_x + slot.left as f32;
    let y = baseline_y - slot.top as f32;
    let w = slot.pixel_size.0 as f32;
    let h = slot.pixel_size.1 as f32;
    append_textured_quad(cell_verts, x, y, w, h, &slot, fg_rgba, bg_rgba);
}

/// Append a panel quad with the same vertex layout used by
/// `Renderer::append_panel_to_verts`. Duplicated here (rather than
/// delegating through the renderer) so this module can be tested
/// without a Metal device.
fn append_panel(
    verts: &mut Vec<PanelVertex>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    corner_radius: f32,
    rgba: u32,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    verts.reserve(VERTICES_PER_PANEL);
    let color = rgba_to_premultiplied(rgba);
    let v = |dx: f32, dy: f32| PanelVertex {
        pos_x: x + dx * width,
        pos_y: y + dy * height,
        local_x: dx,
        local_y: dy,
        size_x: width,
        size_y: height,
        color,
        corner_radius,
        _pad: [0.0; 3],
    };
    verts.push(v(0.0, 0.0));
    verts.push(v(1.0, 0.0));
    verts.push(v(0.0, 1.0));
    verts.push(v(1.0, 0.0));
    verts.push(v(1.0, 1.0));
    verts.push(v(0.0, 1.0));
}

/// Append a textured quad sourcing from `slot.uv_origin` / `uv_size`.
/// Uses the cell pipeline's vertex layout — `fg` tints the alpha mask
/// (UI glyphs and template icons), `bg` shows through transparent
/// regions of the glyph.
#[allow(clippy::too_many_arguments)]
fn append_textured_quad(
    verts: &mut Vec<CellVertex>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    slot: &UISlot,
    fg_rgba: u32,
    bg_rgba: u32,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    verts.reserve(VERTICES_PER_CELL);
    let fg = rgba_to_float(fg_rgba);
    let bg = rgba_to_float(bg_rgba);
    let (u0, v0) = slot.uv_origin;
    let (uw, vh) = slot.uv_size;
    let v = |dx: f32, dy: f32| CellVertex {
        pos_x: x + dx * width,
        pos_y: y + dy * height,
        uv_x: u0 + dx * uw,
        uv_y: v0 + dy * vh,
        fg,
        bg,
        is_color: 0.0,
        _pad: [0.0; 3],
    };
    verts.push(v(0.0, 0.0));
    verts.push(v(1.0, 0.0));
    verts.push(v(0.0, 1.0));
    verts.push(v(1.0, 0.0));
    verts.push(v(1.0, 1.0));
    verts.push(v(0.0, 1.0));
}

/// Render `text` using the terminal monospace font (cell glyph atlas).
/// Each character advances by `cell_w_px` (×2 for wide glyphs).
/// Truncates with "…" when the run exceeds `max_w`.
#[allow(clippy::too_many_arguments)]
fn emit_mono_text_run(
    ctx: &mut HeaderDrawContext<'_>,
    text: &str,
    start_x: f32,
    baseline_y: f32,
    max_w: f32,
    fg_rgba: u32,
    bg_rgba: u32,
    cell_verts: &mut Vec<CellVertex>,
) {
    if text.is_empty() || max_w <= 0.0 || ctx.cell_w_px <= 0.0 {
        return;
    }
    let cell_w = ctx.cell_w_px;
    let ellipsis_w = cell_w * 3.0; // "…" is one wide char but reserve 3 cells for safety
    let fg = rgba_to_float(fg_rgba);
    let bg = rgba_to_float(bg_rgba);

    let mut pen_x = start_x;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let key = GlyphKey {
            codepoint: ch as u32,
            bold: false,
            italic: false,
        };
        ctx.atlas.ensure(key, false);
        let info = ctx.atlas.lookup(key).copied();
        let (uv_origin, uv_size, wide, is_color) = match info {
            Some(g) => (g.uv_origin, g.uv_size, g.wide, g.is_color),
            None => {
                i += 1;
                pen_x += cell_w;
                continue;
            }
        };
        let span = if wide { cell_w * 2.0 } else { cell_w };

        // Check for truncation.
        let remaining_w = if i == chars.len() - 1 {
            0.0
        } else if chars.len() - i <= 2 {
            ellipsis_w
        } else {
            ellipsis_w + span
        };
        if pen_x + span + remaining_w > start_x + max_w && i < chars.len() - 1 {
            // Emit ellipsis and stop.
            let ell_key = GlyphKey {
                codepoint: '…' as u32,
                bold: false,
                italic: false,
            };
            ctx.atlas.ensure(ell_key, false);
            if let Some(ell_info) = ctx.atlas.lookup(ell_key).copied() {
                emit_glyph_info_quad(
                    cell_verts,
                    pen_x,
                    baseline_y,
                    ctx.cell_w_px,
                    ctx.cell_h_px,
                    &ell_info,
                    fg,
                    bg,
                );
            }
            break;
        }

        emit_glyph_info_quad(
            cell_verts,
            pen_x,
            baseline_y,
            ctx.cell_w_px,
            ctx.cell_h_px,
            &GlyphInfo {
                uv_origin,
                uv_size,
                pixel_size: (ctx.cell_w_px as u32, ctx.cell_h_px as u32),
                wide,
                is_color,
            },
            fg,
            bg,
        );

        pen_x += span;
        i += 1;
    }
}

/// Emit a single cell-glyph quad using a `GlyphInfo` (cell atlas lookup).
#[allow(clippy::too_many_arguments)]
fn emit_glyph_info_quad(
    verts: &mut Vec<CellVertex>,
    x: f32,
    baseline_y: f32,
    cell_w_px: f32,
    cell_h_px: f32,
    glyph: &GlyphInfo,
    fg: [f32; 4],
    bg: [f32; 4],
) {
    verts.reserve(VERTICES_PER_CELL);
    let w = if glyph.wide {
        cell_w_px * 2.0
    } else {
        cell_w_px
    };
    let h = cell_h_px;
    let y = baseline_y - h; // cell top = baseline - cell_height
    let (u0, v0) = glyph.uv_origin;
    let (uw, vh) = glyph.uv_size;
    let is_color = if glyph.is_color { 1.0 } else { 0.0 };
    let v = |dx: f32, dy: f32| CellVertex {
        pos_x: x + dx * w,
        pos_y: y + dy * h,
        uv_x: u0 + dx * uw,
        uv_y: v0 + dy * vh,
        fg,
        bg,
        is_color,
        _pad: [0.0; 3],
    };
    verts.push(v(0.0, 0.0));
    verts.push(v(1.0, 0.0));
    verts.push(v(0.0, 1.0));
    verts.push(v(1.0, 0.0));
    verts.push(v(1.0, 1.0));
    verts.push(v(0.0, 1.0));
}

/// Non-premultiplied RGBA float — matches the cell pipeline's tint
/// expectations (cell_fragment does `mix(bg, fg, sample.a)`, so
/// premultiplication isn't applied there).
fn rgba_to_float(rgba: u32) -> [f32; 4] {
    [
        ((rgba >> 24) & 0xFF) as f32 / 255.0,
        ((rgba >> 16) & 0xFF) as f32 / 255.0,
        ((rgba >> 8) & 0xFF) as f32 / 255.0,
        (rgba & 0xFF) as f32 / 255.0,
    ]
}

fn rgba_to_premultiplied(rgba: u32) -> [f32; 4] {
    let a = (rgba & 0xFF) as f32 / 255.0;
    [
        ((rgba >> 24) & 0xFF) as f32 / 255.0 * a,
        ((rgba >> 16) & 0xFF) as f32 / 255.0 * a,
        ((rgba >> 8) & 0xFF) as f32 / 255.0 * a,
        a,
    ]
}
