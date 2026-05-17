//! C FFI for the mock TTY. All symbols are `bt_mock_tty_*`.

use std::ffi::{c_char, c_void};

use crate::mock_tty::programs::raw_sink::RawSink;
use crate::mock_tty::MockTty;
use crate::mock_tty::OutputCallback;

pub struct BtMockTty {
    inner: MockTty,
}

// NOTE: kept as a doc-only alias. The actual FFI symbol uses the function
// pointer type inline below so cbindgen emits a usable C function pointer
// (cbindgen renders `Option<TypeAlias>` as an opaque struct).
// pub type BtMockTtyOutputCallback = unsafe extern "C" fn(*const u8, usize, *mut c_void);

/// # Safety
/// `opts_json` must be either null or point to a NUL-terminated UTF-8 string
/// owned by the caller for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn bt_mock_tty_create(
    program: u32,
    _opts_json: *const c_char,
) -> *mut BtMockTty {
    #[allow(clippy::match_single_binding)] // expanded by tasks 5–7
    let prog: Box<dyn crate::mock_tty::program::Program> = match program {
        _ => Box::new(RawSink::new()),
    };
    let mut inner = MockTty::new(prog);
    inner.program.boot(&mut inner.output);
    Box::into_raw(Box::new(BtMockTty { inner }))
}

/// # Safety
/// `h` must be a pointer returned by `bt_mock_tty_create` that has not been freed.
#[no_mangle]
pub unsafe extern "C" fn bt_mock_tty_free(h: *mut BtMockTty) {
    if h.is_null() {
        return;
    }
    drop(Box::from_raw(h));
}

/// # Safety
/// `h` must be a valid, non-freed handle.
#[no_mangle]
pub unsafe extern "C" fn bt_mock_tty_set_output_callback(
    h: *mut BtMockTty,
    cb: Option<unsafe extern "C" fn(*const u8, usize, *mut c_void)>,
    user_data: *mut c_void,
) {
    if h.is_null() {
        return;
    }
    let h = &mut *h;
    h.inner
        .set_callback(cb.map(|func| OutputCallback { func, user_data }));
    if !h.inner.output.is_empty() {
        h.inner.flush();
    }
}

/// # Safety
/// `h` must be valid; `bytes` must point to at least `len` bytes (or be null when len == 0).
#[no_mangle]
pub unsafe extern "C" fn bt_mock_tty_write(h: *mut BtMockTty, bytes: *const u8, len: usize) -> i32 {
    if h.is_null() {
        return -1;
    }
    if len == 0 {
        return 0;
    }
    if bytes.is_null() {
        return -1;
    }
    let slice = std::slice::from_raw_parts(bytes, len);
    (*h).inner.write_input(slice);
    0
}

/// # Safety
/// `h` must be valid.
#[no_mangle]
pub unsafe extern "C" fn bt_mock_tty_resize(h: *mut BtMockTty, cols: u16, rows: u16) {
    if h.is_null() {
        return;
    }
    (*h).inner.resize(cols.max(1), rows.max(1));
}

/// # Safety
/// `h` must be valid.
#[no_mangle]
pub unsafe extern "C" fn bt_mock_tty_tick(h: *mut BtMockTty, now_ms: u64) {
    if h.is_null() {
        return;
    }
    (*h).inner.tick(now_ms);
}
