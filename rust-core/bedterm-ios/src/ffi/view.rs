//! FFI surface — `BtIosMetalInputView` accessors.
//!
//! Resolve, query, and drive the Metal-backed input view embedded in
//! the VC. Bytes flow in via `bt_ios_view_feed_bytes`; outbound PTY
//! bytes (hardware keyboard, IME) flow out through the
//! `bt_ios_view_set_on_send` callback; resize notifications come via
//! `bt_ios_view_set_on_resize`.

#![cfg(target_os = "ios")]

use crate::metal_view;

/// Resolve the `BtIosMetalInputView *` embedded inside a VC returned by
/// `bt_ios_create_vc`. Returns NULL when the view didn't construct
/// (headless tests).
///
/// # Safety
/// Main thread. `vc_ptr` must be a live VC pointer.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_vc_metal_view(
    vc_ptr: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    if vc_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let vc_obj = vc_ptr as *mut objc2::runtime::AnyObject;
    let mv: *const metal_view::BtIosMetalInputView = objc2::msg_send![&*vc_obj, btIosMetalView];
    mv as *mut std::ffi::c_void
}

/// Feed raw terminal bytes into the `BtIosMetalInputView`'s owned grid.
/// `view_ptr` must be a +1 retained `BtIosMetalInputView *` (e.g. obtained
/// from the host VC). The call is a no-op when any argument is null / empty.
///
/// # Safety
/// Must be called on the main thread. `view_ptr` must point to a live
/// `BtIosMetalInputView`. `bytes` must be valid for `len` bytes for the
/// duration of the call.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_view_feed_bytes(
    view_ptr: *mut std::ffi::c_void,
    bytes: *const u8,
    len: usize,
) {
    if view_ptr.is_null() || bytes.is_null() || len == 0 {
        return;
    }
    let view = &*(view_ptr as *const metal_view::BtIosMetalInputView);
    let slice = std::slice::from_raw_parts(bytes, len);
    view.feed_bytes(slice);
}

/// Cell pixel size (width, height) as reported by the view's renderer
/// atlas. Returns `(0, 0)` if the view has no live renderer (headless
/// tests). Useful for Swift / tests to size scroll surfaces against the
/// authoritative glyph metrics.
///
/// # Safety
/// Same contract as `bt_ios_view_feed_bytes`.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_view_cell_size_px(
    view_ptr: *mut std::ffi::c_void,
    out_w: *mut u32,
    out_h: *mut u32,
) {
    if view_ptr.is_null() {
        return;
    }
    let view = &*(view_ptr as *const metal_view::BtIosMetalInputView);
    let (w, h) = view.cell_pixel_size();
    if !out_w.is_null() {
        *out_w = w;
    }
    if !out_h.is_null() {
        *out_h = h;
    }
}

/// Install a C-callback PTY sink on the metal view. Replaces any prior
/// sink. Pass `cb = None` (NULL) to clear.
///
/// # Safety
/// Main thread. `view_ptr` must be a live `BtIosMetalInputView *`. The
/// `cb` + `ctx` must remain valid until cleared or the view is released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_view_set_on_send(
    view_ptr: *mut std::ffi::c_void,
    // Inline the bare-fn type so cbindgen emits a nullable C function
    // pointer (it doesn't unwrap `Option<TypeAlias>` — see ffi/vc.rs).
    cb: Option<unsafe extern "C" fn(ctx: *mut std::ffi::c_void, bytes: *const u8, len: usize)>,
    ctx: *mut std::ffi::c_void,
) {
    if view_ptr.is_null() {
        return;
    }
    let view = &*(view_ptr as *const metal_view::BtIosMetalInputView);
    match cb {
        Some(cb) => {
            let ctx_addr = ctx as usize;
            view.set_on_send(Box::new(move |bytes: &[u8]| {
                cb(
                    ctx_addr as *mut std::ffi::c_void,
                    bytes.as_ptr(),
                    bytes.len(),
                );
            }));
        }
        None => {
            view.set_on_send(Box::new(|_bytes: &[u8]| {}));
        }
    }
}

/// Install a resize-notification callback on the metal view. The
/// callback fires from `layoutSubviews` whenever the renderer-derived
/// `(cols, rows)` differs from the previously-reported value. Pass
/// `cb = None` (NULL) to clear.
///
/// # Safety
/// Main thread. `view_ptr` must be a live `BtIosMetalInputView *`.
/// The `cb` + `ctx` must remain valid until cleared or the view is
/// released.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_view_set_on_resize(
    view_ptr: *mut std::ffi::c_void,
    cb: Option<unsafe extern "C" fn(ctx: *mut std::ffi::c_void, cols: u16, rows: u16)>,
    ctx: *mut std::ffi::c_void,
) {
    if view_ptr.is_null() {
        return;
    }
    let view = &*(view_ptr as *const metal_view::BtIosMetalInputView);
    match cb {
        Some(cb) => {
            let ctx_addr = ctx as usize;
            view.set_on_resize(Some(Box::new(move |cols: u16, rows: u16| {
                cb(ctx_addr as *mut std::ffi::c_void, cols, rows);
            })));
        }
        None => {
            view.set_on_resize(None);
        }
    }
}

/// Read the most recent `(cols, rows)` the metal view derived from its
/// bounds + cell pixel size. Both out-params may be NULL. Returns
/// `(0, 0)` before the first layout pass.
///
/// # Safety
/// Main thread. `view_ptr` must be a live `BtIosMetalInputView *`.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_view_grid_dim(
    view_ptr: *mut std::ffi::c_void,
    out_cols: *mut u16,
    out_rows: *mut u16,
) {
    if view_ptr.is_null() {
        return;
    }
    let view = &*(view_ptr as *const metal_view::BtIosMetalInputView);
    let (c, r) = view.grid_dim();
    if !out_cols.is_null() {
        *out_cols = c;
    }
    if !out_rows.is_null() {
        *out_rows = r;
    }
}
