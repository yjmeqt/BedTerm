//! C ABI surface. All exported symbols are `bt_` prefixed.
//!
//! Lifetime contract:
//! - `bt_term_new` returns an owned handle; caller must `bt_term_free` exactly once.
//! - All other accessors borrow out POD-by-value plus opaque pointers
//!   into per-handle scratch (e.g. `block_string_scratch`); Swift must
//!   copy out before the next mutating call.

use crate::snapshot::GridSnapshot;
use crate::term::{BtRgb24, Palette, Terminal};

pub struct BtTerm {
    inner: Terminal,
    /// Holds the most recent renderer-side snapshot so callers of
    /// [`Self::snapshot_for_renderer`] get a stable borrow until the
    /// next mutating call.
    cached: Option<GridSnapshot>,
    /// Scratch for any block-string outparam (command / cwd). Pointer
    /// valid only until the next mutating call OR the next block-string
    /// read.
    block_string_scratch: Vec<u8>,
}

impl BtTerm {
    /// Internal helper for the renderer module — produces a fresh snapshot
    /// without going through any FFI ceremony.
    pub(crate) fn snapshot_for_renderer(&mut self) -> &GridSnapshot {
        let snap = self.inner.snapshot();
        self.cached = Some(snap);
        self.cached.as_ref().unwrap()
    }

    pub(crate) fn inner_ref(&self) -> &crate::term::Terminal {
        &self.inner
    }

    pub(crate) fn block_string_scratch_mut(&mut self) -> &mut Vec<u8> {
        &mut self.block_string_scratch
    }
}

#[no_mangle]
pub extern "C" fn bt_term_new(cols: u16, rows: u16) -> *mut BtTerm {
    let cols = cols.max(1);
    let rows = rows.max(1);
    Box::into_raw(Box::new(BtTerm {
        inner: Terminal::new(cols, rows),
        cached: None,
        block_string_scratch: Vec::new(),
    }))
}

/// # Safety
/// `h` must be a pointer returned by `bt_term_new` that has not yet been freed.
#[no_mangle]
pub unsafe extern "C" fn bt_term_free(h: *mut BtTerm) {
    if h.is_null() {
        return;
    }
    drop(Box::from_raw(h));
}

/// # Safety
/// `h` must be a valid, non-freed handle. `bytes` must point to at least `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn bt_term_feed(h: *mut BtTerm, bytes: *const u8, len: usize) {
    if h.is_null() || bytes.is_null() || len == 0 {
        return;
    }
    let term = &mut *h;
    term.cached = None;
    let slice = std::slice::from_raw_parts(bytes, len);
    term.inner.feed(slice);
}

/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_resize(h: *mut BtTerm, cols: u16, rows: u16) {
    if h.is_null() {
        return;
    }
    let term = &mut *h;
    term.cached = None;
    term.inner.resize(cols.max(1), rows.max(1));
}

/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_scroll_by(h: *mut BtTerm, delta: i32) {
    if h.is_null() {
        return;
    }
    let term = &mut *h;
    term.cached = None;
    term.inner.scroll_by(delta);
}

/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_scroll_to_bottom(h: *mut BtTerm) {
    if h.is_null() {
        return;
    }
    let term = &mut *h;
    term.cached = None;
    term.inner.scroll_to_bottom();
}

/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_scroll_offset(h: *const BtTerm) -> u32 {
    if h.is_null() {
        return 0;
    }
    (*h).inner.scroll_offset()
}

/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_scrollback_lines(h: *const BtTerm) -> u32 {
    if h.is_null() {
        return 0;
    }
    (*h).inner.scrollback_lines()
}

/// Return the terminal's current mode flags, packed as `BT_MODE_*` bits
/// defined in `term.rs`. Swift mirrors the layout in `BedTermMode`.
///
/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_mode(h: *const BtTerm) -> u32 {
    if h.is_null() {
        return 0;
    }
    (*h).inner.mode()
}

/// Cursor's current grid line on the active screen. Swift records this on
/// each block boundary so Block view can later snapshot a row range that
/// covers the block.
///
/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_current_line(h: *const BtTerm) -> i32 {
    if h.is_null() {
        return 0;
    }
    (*h).inner.current_line()
}

/// Grid-absolute line index of the screen's bottom row. Block view uses
/// this as the body's upper bound for running blocks so that TUI cells
/// drawn *below* the cursor via cursor-positioning escapes (claude / fzf
/// / gum) stay visible.
///
/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_screen_bottom_line(h: *const BtTerm) -> i32 {
    if h.is_null() {
        return 0;
    }
    (*h).inner.screen_bottom_line()
}

/// Flat C view of a `Palette`. 18 × `BtRgb24` = 54 bytes (no padding —
/// `#[repr(C)]` `BtRgb24` is 3 × u8). Swift passes a pointer; Rust copies in.
#[repr(C)]
pub struct BtPaletteView {
    pub default_fg: BtRgb24,
    pub default_bg: BtRgb24,
    pub ansi: [BtRgb24; 16],
}

/// # Safety
/// `h` must be a valid, non-freed handle. `palette` must point to a valid,
/// aligned `BtPaletteView`, or be null (a null palette is a no-op).
pub unsafe fn bt_term_set_palette(h: *mut BtTerm, palette: *const BtPaletteView) {
    if h.is_null() || palette.is_null() {
        return;
    }
    let term = &mut *h;
    let p = &*palette;
    // Invalidate the cached snapshot — old RGBA values no longer apply.
    term.cached = None;
    term.inner.set_palette(Palette {
        default_fg: p.default_fg,
        default_bg: p.default_bg,
        ansi: p.ansi,
    });
}
