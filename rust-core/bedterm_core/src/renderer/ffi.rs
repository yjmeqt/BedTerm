//! C ABI for the renderer.

use std::os::raw::c_int;

use crate::ffi::BtTerm;
use crate::renderer::Renderer;
use crate::snapshot::CellSnapshot;

pub struct BtRenderer {
    pub(crate) inner: Renderer,
}

/// # Safety
/// `mtl_device` and `mtl_queue` must be non-null `id<MTLDevice>` /
/// `id<MTLCommandQueue>` pointers. They are borrowed for the renderer's
/// lifetime; the caller (Swift) retains them.
pub unsafe fn bt_renderer_new(
    mtl_device: *const std::ffi::c_void,
    mtl_queue: *const std::ffi::c_void,
) -> *mut BtRenderer {
    match Renderer::from_ptrs(mtl_device, mtl_queue) {
        Some(inner) => Box::into_raw(Box::new(BtRenderer { inner })),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `r` must be a pointer returned by `bt_renderer_new` not yet freed.
pub unsafe fn bt_renderer_free(r: *mut BtRenderer) {
    if !r.is_null() {
        drop(Box::from_raw(r));
    }
}

/// # Safety
/// `r` must be a live `BtRenderer` pointer.
pub unsafe fn bt_renderer_set_font(r: *mut BtRenderer, pixel_size: f32, device_pixel_ratio: f32) {
    if r.is_null() {
        return;
    }
    (*r).inner.set_font(pixel_size, device_pixel_ratio);
}

/// Write the renderer's current cell size (in PIXELS, scaled by the dpr
/// passed to `bt_renderer_set_font`) into `*out_w` and `*out_h`. Swift
/// divides by its display scale to obtain the point-space cell size used
/// for laying out CALayer overlays (cursor, selection) — keeping them
/// pixel-aligned with the glyphs the renderer paints.
///
/// # Safety
/// `r` must be a live `BtRenderer`. `out_w` and `out_h` must be valid
/// pointers to `u32` slots the caller owns.
pub unsafe fn bt_renderer_cell_pixel_size(r: *const BtRenderer, out_w: *mut u32, out_h: *mut u32) {
    if r.is_null() || out_w.is_null() || out_h.is_null() {
        return;
    }
    let (w, h) = (*r).inner.atlas.cell_px;
    *out_w = w;
    *out_h = h;
}

/// # Safety
/// `r` must be a live `BtRenderer` pointer, or null (null is a no-op).
/// Components are clamped to `[0, 1]` downstream by Metal; values outside
/// that range are tolerated.
pub unsafe fn bt_renderer_set_clear_color(
    r: *mut BtRenderer,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
) {
    if r.is_null() {
        return;
    }
    (*r).inner.set_clear_color(red, green, blue, alpha);
}

/// # Safety
/// `r` must be a live `BtRenderer`. `term` must be a live `BtTerm` or null
/// (null is treated as "no terminal yet"). `drawable_texture` must be a
/// live `id<MTLTexture>`.
pub unsafe fn bt_renderer_draw(
    r: *mut BtRenderer,
    term: *const BtTerm,
    drawable_texture: *const std::ffi::c_void,
    viewport_width_px: u32,
    viewport_height_px: u32,
    time_seconds: f64,
) -> c_int {
    if r.is_null() {
        return -1;
    }
    (*r).inner.draw(
        term,
        drawable_texture,
        viewport_width_px,
        viewport_height_px,
        time_seconds,
    )
}

/// Render an arbitrary cell array — used by Block view to draw each
/// block's body (either a frozen snapshot of a sealed block or a fresh
/// row-range snapshot of a running block) through the same Metal pipeline
/// the main terminal view uses.
///
/// `cells_len` must equal `cols as usize * rows as usize`. `cells` may be
/// null with `cells_len == 0` for an empty draw (clears the viewport).
///
/// # Safety
/// `r` must be a live `BtRenderer`. `cells` (when non-null) must point to
/// `cells_len` valid `CellSnapshot` values for the duration of the call.
/// `drawable_texture` must be a live `id<MTLTexture>`.
#[allow(clippy::too_many_arguments)]
pub unsafe fn bt_renderer_draw_cells(
    r: *mut BtRenderer,
    cells: *const CellSnapshot,
    cells_len: usize,
    cols: u16,
    rows: u16,
    drawable_texture: *const std::ffi::c_void,
    viewport_width_px: u32,
    viewport_height_px: u32,
    time_seconds: f64,
) -> c_int {
    if r.is_null() {
        return -1;
    }
    (*r).inner.draw_cells(
        cells,
        cells_len,
        cols,
        rows,
        drawable_texture,
        viewport_width_px,
        viewport_height_px,
        time_seconds,
    )
}

/// Register a host-supplied font face (TTF / TTC / OTF bytes) and
/// promote it to the primary terminal family. Used by Swift to wire
/// iOS's SF Mono / Menlo into the Rust rasterizer at app launch.
///
/// **The family name is sourced from the sfnt `name` table by fontdb**,
/// not from the caller — this closes the trap where iOS's CTFont
/// surface name (`.AppleSystemUIFontMonospaced`) doesn't match what
/// the font file actually declares (`SF Mono`), which would otherwise
/// make `Family::Name(...)` at shape time silently fall through to
/// the bundled JetBrains Mono.
///
/// The resolved family is written back into `out_family_ptr` (up to
/// `out_family_capacity` bytes, no NUL terminator) so the caller can
/// log it. If `out_family_ptr` is null or capacity is 0, registration
/// still happens and the return value just reports the length that
/// would have been written.
///
/// Returns the family name length on success (≥ 0), or `-1` if
/// `bytes_ptr` is null / `bytes_len == 0`, or fontdb couldn't extract
/// any face from the bytes.
///
/// # Safety
/// `bytes_ptr` must point to `bytes_len` readable bytes for the
/// duration of the call. `out_family_ptr`, if non-null, must point to
/// `out_family_capacity` writable bytes.
pub unsafe fn bt_font_register_terminal_face(
    bytes_ptr: *const u8,
    bytes_len: usize,
    out_family_ptr: *mut u8,
    out_family_capacity: usize,
) -> c_int {
    if bytes_ptr.is_null() || bytes_len == 0 {
        return -1;
    }
    let data = std::slice::from_raw_parts(bytes_ptr, bytes_len).to_vec();
    let Some(family) = crate::renderer::font_system::register_terminal_face(data) else {
        return -1;
    };
    let bytes = family.as_bytes();
    if !out_family_ptr.is_null() && out_family_capacity > 0 {
        let n = bytes.len().min(out_family_capacity);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_family_ptr, n);
    }
    bytes.len() as c_int
}

/// Register an **auxiliary** font face (Bold / Italic / BoldItalic
/// companions of the primary terminal family). Unlike
/// `bt_font_register_terminal_face`, this does **not** promote the
/// face to the primary family — cosmic-text resolves the weight/style
/// variant by matching `Attrs::weight` / `Attrs::style` against fontdb
/// after the family lookup. Use this for every non-Regular Menlo cut.
///
/// Returns 0 on success, -1 if `bytes_ptr` is null / `bytes_len == 0`,
/// or fontdb couldn't extract any face from the bytes.
///
/// # Safety
/// `bytes_ptr` must point to `bytes_len` readable bytes for the
/// duration of the call.
pub unsafe fn bt_font_register_aux_face(bytes_ptr: *const u8, bytes_len: usize) -> c_int {
    if bytes_ptr.is_null() || bytes_len == 0 {
        return -1;
    }
    let data = std::slice::from_raw_parts(bytes_ptr, bytes_len).to_vec();
    if crate::renderer::font_system::register_aux_face(data) {
        0
    } else {
        -1
    }
}
