//! C ABI surface. All exported symbols are `bt_` prefixed.
//!
//! Lifetime contract:
//! - `bt_term_new` returns an owned handle; caller must `bt_term_free` exactly once.
//! - `bt_term_snapshot` borrows-out cell pointers valid until the next call to
//!   `bt_term_feed` / `bt_term_resize` / `bt_term_snapshot` / `bt_term_free`,
//!   OR until `bt_term_snapshot_release` is called — whichever happens first.
//!   Swift must copy out cells before mutating the terminal.

use std::ffi::c_char;
use std::os::raw::c_int;

use crate::snapshot::{CellSnapshot, GridSnapshot};
use crate::term::{BtRgb24, Palette, Terminal};

#[repr(C)]
pub struct BtSnapshotView {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    /// Equals `rows` when the cursor is scrolled off-screen.
    pub cursor_row: u16,
    /// 0 = at live bottom; positive = N rows into scrollback.
    pub display_offset: u32,
    pub cells: *const CellSnapshot,
    pub cell_count: usize,
}

pub struct BtTerm {
    inner: Terminal,
    cached: Option<GridSnapshot>,
    /// Scratch for any block-string outparam (command / cwd). Same
    /// invalidation contract as `bt_term_snapshot`: pointer valid only
    /// until the next mutating call OR the next block-string read.
    block_string_scratch: Vec<u8>,
    /// Cached snapshot for the most recent frozen-block snapshot view we
    /// handed out. Pointer in `BtSnapshotView` is valid until the next
    /// snapshot read / mutating call / explicit release.
    block_snapshot_cached: Option<GridSnapshot>,
}

impl BtTerm {
    /// Internal helper for the renderer module — produces a fresh snapshot
    /// without going through the cached-pointer FFI ceremony.
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

    pub(crate) fn set_block_snapshot_cached(&mut self, snap: crate::snapshot::GridSnapshot) {
        self.block_snapshot_cached = Some(snap);
    }

    pub(crate) fn clear_block_snapshot_cached(&mut self) {
        self.block_snapshot_cached = None;
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
        block_snapshot_cached: None,
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
/// `h` must be a valid, non-freed handle. `out` must be a valid pointer to a `BtSnapshotView`.
/// The cell pointer in `*out` is valid until the next mutating call or `bt_term_snapshot_release`.
#[no_mangle]
pub unsafe extern "C" fn bt_term_snapshot(h: *mut BtTerm, out: *mut BtSnapshotView) -> c_int {
    if h.is_null() || out.is_null() {
        return -1;
    }
    let term = &mut *h;
    let snap = term.inner.snapshot();
    let view = BtSnapshotView {
        cols: snap.cols,
        rows: snap.rows,
        cursor_col: snap.cursor_col,
        cursor_row: snap.cursor_row,
        display_offset: snap.display_offset,
        cells: snap.cells.as_ptr(),
        cell_count: snap.cells.len(),
    };
    term.cached = Some(snap);
    *out = view;
    0
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

/// Snapshot a row range from the active screen + scrollback. Same lifetime
/// contract as `bt_term_snapshot` — the cell pointer in `*out` is valid
/// until the next mutating call. `start_line` inclusive, `end_line`
/// exclusive; values outside the grid extent are clamped.
///
/// # Safety
/// `h` must be a valid, non-freed handle. `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn bt_term_snapshot_range(
    h: *mut BtTerm,
    start_line: i32,
    end_line: i32,
    out: *mut BtSnapshotView,
) -> c_int {
    if h.is_null() || out.is_null() {
        return -1;
    }
    let term = &mut *h;
    let snap = term.inner.snapshot_range(start_line, end_line);
    let view = BtSnapshotView {
        cols: snap.cols,
        rows: snap.rows,
        cursor_col: snap.cursor_col,
        cursor_row: snap.cursor_row,
        display_offset: snap.display_offset,
        cells: snap.cells.as_ptr(),
        cell_count: snap.cells.len(),
    };
    term.cached = Some(snap);
    *out = view;
    0
}

/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_term_snapshot_release(h: *mut BtTerm) {
    if h.is_null() {
        return;
    }
    let term = &mut *h;
    term.cached = None;
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
#[no_mangle]
pub unsafe extern "C" fn bt_term_set_palette(h: *mut BtTerm, palette: *const BtPaletteView) {
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

/// Attach persistence to a `BtTerm` handle — a convenience shim over
/// `bedterm_persistence_attach` that accepts the opaque `BtTerm *` Swift
/// already owns rather than requiring Swift to materialise a bare `Terminal *`.
///
/// # Safety
/// `h` must be a valid `BtTerm *` returned by `bt_term_new`.
/// `handle`, `snapshot_id`, and `host_id` follow the same safety contract
/// as `bedterm_persistence_attach`.
#[no_mangle]
pub unsafe extern "C" fn bt_term_attach_persistence(
    h: *mut BtTerm,
    handle: *mut crate::persistence::ffi::PersistenceHandle,
    snapshot_id: *const c_char,
    host_id: *const c_char,
) {
    if h.is_null() {
        return;
    }
    // SAFETY: `BtTerm.inner` is the first field; we take a mutable reference
    // to it and forward to `bedterm_persistence_attach` as `*mut Terminal`.
    let term_ptr: *mut Terminal = &mut (*h).inner;
    crate::persistence::ffi::bedterm_persistence_attach(handle, term_ptr, snapshot_id, host_id);
}
