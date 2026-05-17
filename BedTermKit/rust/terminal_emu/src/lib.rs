//! FFI exports for the terminal emulator.
//!
//! Every function follows C ABI conventions:
//! - Null pointers are checked on entry.
//! - Returned pointers must be freed by the paired `_destroy` / `_free` function.
//! - `term_get_grid` returns a snapshot valid until the next mutable call on the
//!   same `TermEmu*`.
//!
//! Thread safety: `TermEmu` is `!Sync`. All calls must happen on the same thread
//! (the main thread in practice, since Metal rendering is main-thread-only).

mod color;
mod grid;
mod input;
mod term;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

pub use color::AnsiPalette;
pub use grid::{Cell, DirtyRange, TerminalGrid};
pub use input::InputBuffer;
pub use term::TermEmu;

// ── Terminal lifecycle ──────────────────────────────────────────────

/// Allocate a new terminal emulator.
/// `scrollback_limit`: max scrollback lines (0 = unbounded).
/// Returns null on allocation failure.
#[no_mangle]
pub extern "C" fn term_create(cols: u16, rows: u16, scrollback_limit: u32) -> *mut TermEmu {
    let limit = if scrollback_limit == 0 {
        None
    } else {
        Some(scrollback_limit as usize)
    };
    let term = TermEmu::new(cols as usize, rows as usize, limit);
    Box::into_raw(Box::new(term))
}

/// Free the terminal.
#[no_mangle]
pub extern "C" fn term_destroy(ptr: *mut TermEmu) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(ptr));
    }
}

// ── Data flow ───────────────────────────────────────────────────────

/// Feed raw bytes (SSH stdout) into the parser.
/// Returns the number of dirty line ranges. Call `term_get_grid` afterwards
/// and inspect its `dirty` / `dirty_count` fields to find which rows to repaint.
#[no_mangle]
pub extern "C" fn term_feed(ptr: *mut TermEmu, bytes: *const u8, len: u32) -> u32 {
    if ptr.is_null() || bytes.is_null() || len == 0 {
        return 0;
    }
    let term = unsafe { &mut *ptr };
    let slice = unsafe { std::slice::from_raw_parts(bytes, len as usize) };
    term.feed(slice)
}

/// Get a read-only snapshot of the terminal grid.
/// The returned pointers are valid until the next `term_feed` or `term_resize`
/// on the same terminal.
#[no_mangle]
pub extern "C" fn term_get_grid(ptr: *const TermEmu) -> TerminalGrid {
    if ptr.is_null() {
        return TerminalGrid::empty();
    }
    let term = unsafe { &*ptr };
    term.grid_snapshot()
}

/// Resize the terminal grid. Marks all rows dirty.
#[no_mangle]
pub extern "C" fn term_resize(ptr: *mut TermEmu, cols: u16, rows: u16) {
    if ptr.is_null() {
        return;
    }
    let term = unsafe { &mut *ptr };
    term.resize(cols as usize, rows as usize);
}

// ── Selection ───────────────────────────────────────────────────────

/// Get selected text as a UTF-8 C string.
/// Caller must free with `term_string_free`.
/// Returns null if no selection or on error.
#[no_mangle]
pub extern "C" fn term_get_selection(ptr: *const TermEmu) -> *mut c_char {
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let term = unsafe { &*ptr };
    match term.selection_text() {
        Some(text) => CString::new(text)
            .ok()
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Free a string returned by the library.
#[no_mangle]
pub extern "C" fn term_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    }
}

// ── Input buffer (Warp block) ───────────────────────────────────────

/// Allocate a new input buffer.
#[no_mangle]
pub extern "C" fn input_buf_create() -> *mut InputBuffer {
    Box::into_raw(Box::new(InputBuffer::new()))
}

/// Free an input buffer.
#[no_mangle]
pub extern "C" fn input_buf_destroy(ptr: *mut InputBuffer) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(ptr));
    }
}

/// Append UTF-8 bytes to the input buffer. Returns new byte count.
#[no_mangle]
pub extern "C" fn input_buf_append(ptr: *mut InputBuffer, bytes: *const u8, len: u32) -> u32 {
    if ptr.is_null() || bytes.is_null() || len == 0 {
        return 0;
    }
    let buf = unsafe { &mut *ptr };
    let slice = unsafe { std::slice::from_raw_parts(bytes, len as usize) };
    buf.append(slice)
}

/// Get the buffer contents as a UTF-8 C string.
/// Caller must free with `term_string_free`.
#[no_mangle]
pub extern "C" fn input_buf_get_text(ptr: *const InputBuffer) -> *mut c_char {
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let buf = unsafe { &*ptr };
    CString::new(buf.as_str())
        .ok()
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

/// Clear the input buffer.
#[no_mangle]
pub extern "C" fn input_buf_clear(ptr: *mut InputBuffer) {
    if ptr.is_null() {
        return;
    }
    let buf = unsafe { &mut *ptr };
    buf.clear();
}

/// Delete the last N bytes from the buffer (backspace).
/// Returns the new byte count.
#[no_mangle]
pub extern "C" fn input_buf_delete_last(ptr: *mut InputBuffer, n: u32) -> u32 {
    if ptr.is_null() {
        return 0;
    }
    let buf = unsafe { &mut *ptr };
    buf.delete_last(n as usize)
}
