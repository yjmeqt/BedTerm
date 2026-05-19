//! C ABI for the single-surface block-list renderer.
//!
//! Lifetime / threading:
//! - `entries` is borrowed for the duration of the call; the host copies
//!   it into a Swift `[BtBlockLayoutEntry]` and passes the buffer base.
//! - Same Metal-safety rules as `bt_renderer_draw`: the texture is
//!   borrowed (no retain transfer); Rust uses ManuallyDrop semantics.

use crate::ffi::BtTerm;
use crate::renderer::ffi::BtRenderer;
use std::os::raw::c_int;

/// One entry per block telling Rust where to paint that block's body in
/// the logical content space. Header chrome is rendered by Swift in a
/// UIScrollView Z-overlay and is NOT drawn here.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BtBlockLayoutEntry {
    /// Matches `Block::id`. Looked up by linear scan over `term.blocks()`.
    pub block_id: u64,
    /// Top-left Y of the BODY (excluding header) in logical content
    /// coordinates (pixels). Swift accumulates header + body heights to
    /// compute this.
    pub body_y_top_px: f32,
    /// Body height in pixels (row_count × cell_height_px).
    pub body_height_px: f32,
}

/// Paint visible block bodies into `texture` for one frame. First
/// visible block clears the viewport; subsequent calls use Load. If no
/// block intersects the viewport, the viewport is still cleared.
///
/// # Safety
/// `r`, `term`, `texture_ptr` must be valid live pointers. `entries`
/// must point to at least `entry_count` `BtBlockLayoutEntry` values
/// (or be null with `entry_count == 0`).
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
    renderer.draw_block_list(
        term_ref,
        texture_ptr,
        viewport_w,
        viewport_h,
        scroll_y_px,
        layout,
    )
}
