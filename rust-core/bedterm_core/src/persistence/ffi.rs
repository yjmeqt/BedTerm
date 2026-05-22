//! C ABI for the persistence module. cbindgen emits these into the
//! generated `bedterm_core.h`. Conventions: pointers may be null;
//! all string pointers are NUL-terminated UTF-8; functions never
//! panic across the FFI boundary (wrapped in `catch_unwind`).

use std::ffi::{c_char, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};

use super::Database;

/// Opaque handle. Allocated by `init`, freed by `close`.
pub struct PersistenceHandle {
    pub(crate) db: Database,
}

/// # Safety
/// `db_path` must be a valid NUL-terminated UTF-8 C string or null.
/// The returned pointer must be freed with `bedterm_persistence_close`.
#[no_mangle]
pub unsafe extern "C" fn bedterm_persistence_init(
    db_path: *const c_char,
) -> *mut PersistenceHandle {
    if db_path.is_null() {
        return std::ptr::null_mut();
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let path = unsafe { CStr::from_ptr(db_path) }.to_str().ok()?;
        let db = Database::open(path).ok()?;
        Some(Box::into_raw(Box::new(PersistenceHandle { db })))
    }));
    result.ok().flatten().unwrap_or(std::ptr::null_mut())
}

/// # Safety
/// `handle` must be a pointer previously returned by
/// `bedterm_persistence_init` and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bedterm_persistence_close(handle: *mut PersistenceHandle) {
    if handle.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        drop(unsafe { Box::from_raw(handle) });
    }));
}
