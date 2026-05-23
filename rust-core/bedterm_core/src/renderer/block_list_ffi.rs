//! C ABI for the single-surface block-list renderer.
//!
//! Lifetime / threading:
//! - `entries` and `headers` are borrowed for the duration of the call;
//!   the host copies them into Swift arrays and passes the buffer base.
//! - String pointers inside `BtBlockHeaderEntry` (`command_utf8`,
//!   `subtitle_utf8`) must stay valid for the call. Rust does not retain
//!   them past the FFI invocation.
//! - Same Metal-safety rules as `bt_renderer_draw`: the texture is
//!   borrowed (no retain transfer); Rust uses ManuallyDrop semantics.

use crate::ffi::BtTerm;
use crate::renderer::ffi::BtRenderer;
use std::os::raw::c_int;

/// One entry per block: the BODY cell region plus the panel chrome
/// rect. Headers are emitted separately via `BtBlockHeaderEntry`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BtBlockLayoutEntry {
    /// Matches `Block::id`. Looked up by linear scan over `term.blocks()`.
    pub block_id: u64,
    /// Top-left Y of the BODY (cells start here) in logical content
    /// coordinates (pixels).
    pub body_y_top_px: f32,
    /// Body height in pixels (row_count × cell_height_px).
    pub body_height_px: f32,
    /// Top-left Y of the PANEL chrome (includes header). The rounded
    /// panel BG paints from this Y down to `panel_y_top_px + panel_height_px`.
    pub panel_y_top_px: f32,
    /// Panel height in pixels (header + body + any inset).
    pub panel_height_px: f32,
    /// Panel left edge X in pixels.
    pub panel_x_left_px: f32,
    /// Panel width in pixels.
    pub panel_width_px: f32,
    /// Panel background RGBA (0xRRGGBBAA, big-endian packed). Pass 0 to
    /// skip panel rendering for this entry (terminal pane fallback).
    pub panel_bg_rgba: u32,
    /// Panel corner radius in pixels.
    pub panel_corner_radius_px: f32,
}

/// One entry per visible block header band. Sticky descriptors use the
/// same struct with `is_sticky = 1` and `header_y_top_px` in
/// **screen-space** (post-scroll) rather than content-space.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BtBlockHeaderEntry {
    pub block_id: u64,
    /// For natural headers: top Y in **content** coords (pre-scroll).
    /// For sticky (is_sticky=1): top Y in **screen** coords.
    pub header_y_top_px: f32,
    pub header_height_px: f32,
    pub panel_x_left_px: f32,
    pub panel_width_px: f32,
    /// UTF-8 bytes of the command string. Nullable when `command_len == 0`.
    pub command_utf8: *const u8,
    pub command_len: u32,
    /// UTF-8 bytes of the subtitle string (e.g. "exit 0 · 1.2s").
    /// Nullable when `subtitle_len == 0`.
    pub subtitle_utf8: *const u8,
    pub subtitle_len: u32,
    /// 0 = no badge; nonzero values mirror the Swift `CLIAgent` enum.
    /// Branded slots known to the icon atlas (Claude=1, Codex=2);
    /// everything else falls back to the generic glyph.
    pub agent_id: u8,
    pub _pad: [u8; 3],
    /// Badge circle fill (0xRRGGBBAA).
    pub badge_tint_rgba: u32,
    /// Command text colour.
    pub command_fg_rgba: u32,
    /// Subtitle text colour.
    pub subtitle_fg_rgba: u32,
    /// Hairline divider rgba. For natural headers, painted ABOVE the
    /// band; for sticky headers, painted BELOW (so the pinned chrome
    /// reads as a section header floating above the scrolling body).
    /// Pass 0 to skip.
    pub divider_rgba: u32,
    /// 1 = this is the pinned sticky band. Drawn last (z-sorted on top)
    /// and given an opaque surface-coloured fill so scrolling body
    /// cells underneath don't bleed through.
    pub is_sticky: u8,
    pub _pad2: [u8; 3],
}

// Pointers in `BtBlockHeaderEntry` are borrowed for the duration of the
// FFI call; Send/Sync attest that crossing the boundary is OK.
unsafe impl Send for BtBlockHeaderEntry {}
unsafe impl Sync for BtBlockHeaderEntry {}

/// Paint visible block bodies + header bands into `texture` for one
/// frame. First visible block clears the viewport; subsequent calls use
/// Load. If no block intersects the viewport, the viewport is still
/// cleared.
///
/// # Safety
/// `r`, `term`, `texture_ptr` must be valid live pointers. `entries`
/// must point to at least `entry_count` `BtBlockLayoutEntry` values
/// (or be null with `entry_count == 0`). Same for `headers` /
/// `header_count`. UTF-8 pointers inside header entries must outlive
/// the call.
#[no_mangle]
pub unsafe extern "C" fn bt_renderer_draw_block_list(
    r: *mut BtRenderer,
    term: *mut BtTerm,
    texture_ptr: *const std::ffi::c_void,
    viewport_w: u32,
    viewport_h: u32,
    scroll_y_px: f32,
    entries: *const BtBlockLayoutEntry,
    entry_count: usize,
    headers: *const BtBlockHeaderEntry,
    header_count: usize,
) -> c_int {
    if r.is_null() || term.is_null() || texture_ptr.is_null() {
        return -1;
    }
    let renderer = &mut (*r).inner;
    let term_ref = &mut *term;
    let layout: &[BtBlockLayoutEntry] = if entries.is_null() || entry_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(entries, entry_count)
    };
    let hdrs: &[BtBlockHeaderEntry] = if headers.is_null() || header_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(headers, header_count)
    };
    renderer.draw_block_list(
        term_ref,
        texture_ptr,
        viewport_w,
        viewport_h,
        scroll_y_px,
        layout,
        hdrs,
    )
}

/// Push current UI font sizes into the renderer. Called by Swift on
/// init and whenever the trait collection changes (Dynamic Type, scale).
/// `*_px` are in **pixels** (point size × screen scale).
///
/// # Safety
/// `r` must be a valid live pointer.
#[no_mangle]
pub unsafe extern "C" fn bt_renderer_set_ui_font_sizes_px(
    r: *mut BtRenderer,
    subheadline_px: f32,
    caption2_px: f32,
    scale: f32,
) -> c_int {
    if r.is_null() {
        return -1;
    }
    (*r).inner
        .set_ui_font_sizes(subheadline_px, caption2_px, scale);
    0
}
