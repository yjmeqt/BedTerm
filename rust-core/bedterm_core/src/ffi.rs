//! C ABI surface. All exported symbols are `bt_` prefixed.
//!
//! Lifetime contract:
//! - `bt_term_new` returns an owned handle; caller must `bt_term_free` exactly once.
//! - `bt_term_snapshot` borrows-out cell pointers valid until the next call to
//!   `bt_term_feed` / `bt_term_resize` / `bt_term_snapshot` / `bt_term_free`,
//!   OR until `bt_term_snapshot_release` is called — whichever happens first.
//!   Swift must copy out cells before mutating the terminal.

use std::os::raw::c_int;

use crate::snapshot::{CellSnapshot, GridSnapshot};
use crate::term::Terminal;

#[repr(C)]
pub struct BtSnapshotView {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub cells: *const CellSnapshot,
    pub cell_count: usize,
}

pub struct BtTerm {
    inner: Terminal,
    cached: Option<GridSnapshot>,
}

impl BtTerm {
    /// Internal helper for the renderer module — produces a fresh snapshot
    /// without going through the cached-pointer FFI ceremony.
    pub(crate) fn snapshot_for_renderer(&mut self) -> &GridSnapshot {
        let snap = self.inner.snapshot();
        self.cached = Some(snap);
        self.cached.as_ref().unwrap()
    }
}

#[no_mangle]
pub extern "C" fn bt_term_new(cols: u16, rows: u16) -> *mut BtTerm {
    let cols = cols.max(1);
    let rows = rows.max(1);
    Box::into_raw(Box::new(BtTerm {
        inner: Terminal::new(cols, rows),
        cached: None,
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
