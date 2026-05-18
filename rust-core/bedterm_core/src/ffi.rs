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
    /// Scratch buffer for the most recently popped OSC 133 event's `attrs`.
    /// The pointer handed across FFI in `BtOsc133Event::attrs` aims here and
    /// is only valid until the next mutating call (next pop / feed / resize /
    /// free) — same lifetime contract as `bt_term_snapshot`'s cell pointer.
    osc133_attrs_scratch: Vec<u8>,
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
        osc133_attrs_scratch: Vec::new(),
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

/// Discriminator values for `BtOsc133Event::kind`. Swift mirrors these in
/// `BedTermOsc133Event`.
pub const BT_OSC133_PROMPT_START: u8 = 0;
pub const BT_OSC133_COMMAND_START: u8 = 1;
pub const BT_OSC133_OUTPUT_START: u8 = 2;
pub const BT_OSC133_COMMAND_END: u8 = 3;

/// One OSC 133 (FinalTerm) shell-integration event. Tagged union with a
/// single-payload field (`exit_code`) that's only meaningful when
/// `kind == COMMAND_END`, plus a borrowed `attrs` slice carrying the raw
/// `key=value;key=value` extension tail.
#[repr(C)]
pub struct BtOsc133Event {
    /// One of the `BT_OSC133_*` constants.
    pub kind: u8,
    /// `1` when the remote shell shipped an exit code; `0` otherwise.
    /// Only meaningful when `kind == BT_OSC133_COMMAND_END`.
    pub has_exit_code: u8,
    /// Padding so `exit_code` is naturally aligned. Caller must ignore.
    pub _reserved: [u8; 2],
    /// Command exit code. Only meaningful when `kind == BT_OSC133_COMMAND_END`
    /// and `has_exit_code != 0`.
    pub exit_code: i32,
    /// UTF-8 bytes of the extension attribute tail — `key=value` pairs
    /// joined by `;`, exactly as the integration emitted them. Null when
    /// no attrs. Borrowed from a scratch buffer inside the `BtTerm`; valid
    /// only until the next mutating call. Caller copies before drainin
    /// further events.
    pub attrs: *const u8,
    pub attrs_len: usize,
}

/// Pop one queued OSC 133 event, if any. Writes into `*out` and returns `1`
/// when an event was popped, `0` when the queue is empty. Drain in a loop
/// after each call to `bt_term_feed`.
///
/// `attrs` in the returned event points into a scratch buffer that the next
/// mutating call overwrites — copy out the bytes before calling pop again.
///
/// # Safety
/// `h` must be a valid, non-freed handle; `out` must point to a writable
/// `BtOsc133Event`.
#[no_mangle]
pub unsafe extern "C" fn bt_term_pop_osc133(h: *mut BtTerm, out: *mut BtOsc133Event) -> u8 {
    if h.is_null() || out.is_null() {
        return 0;
    }
    let term = &mut *h;
    let Some(event) = term.inner.pop_osc133() else {
        return 0;
    };
    use crate::osc133::Osc133Event;
    let (kind, has, code, attrs) = match event {
        Osc133Event::PromptStart { attrs } => (BT_OSC133_PROMPT_START, 0u8, 0, attrs),
        Osc133Event::CommandStart { attrs } => (BT_OSC133_COMMAND_START, 0u8, 0, attrs),
        Osc133Event::OutputStart { attrs } => (BT_OSC133_OUTPUT_START, 0u8, 0, attrs),
        Osc133Event::CommandEnd {
            exit_code: Some(code),
            attrs,
        } => (BT_OSC133_COMMAND_END, 1u8, code, attrs),
        Osc133Event::CommandEnd {
            exit_code: None,
            attrs,
        } => (BT_OSC133_COMMAND_END, 0u8, 0, attrs),
    };
    term.osc133_attrs_scratch = attrs;
    let (attrs_ptr, attrs_len) = if term.osc133_attrs_scratch.is_empty() {
        (std::ptr::null(), 0)
    } else {
        (
            term.osc133_attrs_scratch.as_ptr(),
            term.osc133_attrs_scratch.len(),
        )
    };
    *out = BtOsc133Event {
        kind,
        has_exit_code: has,
        _reserved: [0; 2],
        exit_code: code,
        attrs: attrs_ptr,
        attrs_len,
    };
    1
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
