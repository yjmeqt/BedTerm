//! C ABI for the renderer.

use std::os::raw::c_int;

use crate::ffi::BtTerm;
use crate::renderer::Renderer;

pub struct BtRenderer {
    inner: Renderer,
}

/// # Safety
/// `mtl_device` and `mtl_queue` must be non-null `id<MTLDevice>` /
/// `id<MTLCommandQueue>` pointers. They are borrowed for the renderer's
/// lifetime; the caller (Swift) retains them.
#[no_mangle]
pub unsafe extern "C" fn bt_renderer_new(
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
#[no_mangle]
pub unsafe extern "C" fn bt_renderer_free(r: *mut BtRenderer) {
    if !r.is_null() {
        drop(Box::from_raw(r));
    }
}

/// # Safety
/// `r` must be a live `BtRenderer` pointer.
#[no_mangle]
pub unsafe extern "C" fn bt_renderer_set_font(
    r: *mut BtRenderer,
    pixel_size: f32,
    device_pixel_ratio: f32,
) {
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
#[no_mangle]
pub unsafe extern "C" fn bt_renderer_cell_pixel_size(
    r: *const BtRenderer,
    out_w: *mut u32,
    out_h: *mut u32,
) {
    if r.is_null() || out_w.is_null() || out_h.is_null() {
        return;
    }
    let (w, h) = (*r).inner.atlas.cell_px;
    *out_w = w;
    *out_h = h;
}

/// # Safety
/// `r` must be a live `BtRenderer`. Components are clamped to `[0, 1]`
/// downstream by Metal; values outside that range are tolerated.
#[no_mangle]
pub unsafe extern "C" fn bt_renderer_set_clear_color(
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
#[no_mangle]
pub unsafe extern "C" fn bt_renderer_draw(
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
