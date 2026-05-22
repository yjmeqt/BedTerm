//! C ABI for the persistence module. cbindgen emits these into the
//! generated `bedterm_core.h`. Conventions: pointers may be null;
//! all string pointers are NUL-terminated UTF-8; functions never
//! panic across the FFI boundary (wrapped in `catch_unwind`).

use std::ffi::{c_char, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};

use super::Database;
use crate::term::Terminal;

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

/// Wire `terminal` to the persistence database held in `handle`, scoped to
/// `snapshot_id`. This function also ensures a `snapshots` row exists for
/// `snapshot_id` (idempotent — harmlessly fails with a constraint error if
/// the row was already inserted).
///
/// After this call, every `CommandFinished` event processed by `terminal`
/// inserts a row into the `blocks` table. Blocks sealed by Ctrl-C
/// (Precmd-over-running-command path) are also persisted, with `exit_code`
/// set to NULL.
///
/// # Safety
///
/// - `handle`, `terminal`, `snapshot_id`, and `host_id` must all be valid
///   non-null pointers.
/// - `snapshot_id` and `host_id` must be NUL-terminated UTF-8 C strings.
/// - The caller MUST keep `handle` alive until `terminal` is freed. The
///   persistence sink installed by this function holds a raw pointer into
///   `handle.db` — if the handle is freed while the terminal is still live,
///   any subsequent block finalization will access freed memory.
///
/// The intended call order is:
///   1. `bedterm_persistence_attach(handle, term, sid, hid)` — on session open.
///   2. Feed PTY bytes to `term` via `bedterm_feed` as usual.
///   3. Free `term` (e.g. `bedterm_free`).
///   4. `bedterm_persistence_close(handle)` — after the terminal is gone.
#[no_mangle]
pub unsafe extern "C" fn bedterm_persistence_attach(
    handle: *mut PersistenceHandle,
    terminal: *mut Terminal,
    snapshot_id: *const c_char,
    host_id: *const c_char,
) {
    if handle.is_null() || terminal.is_null() || snapshot_id.is_null() || host_id.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let h = unsafe { &mut *handle };
        let term = unsafe { &mut *terminal };
        let sid = unsafe { CStr::from_ptr(snapshot_id) }
            .to_str()
            .unwrap_or("");
        let hid = unsafe { CStr::from_ptr(host_id) }.to_str().unwrap_or("");
        // Insert snapshot row idempotently — duplicate key is silently
        // ignored so re-attaching to an existing session is safe.
        let row = super::SnapshotRow {
            id: sid.to_string(),
            host_id: hid.to_string(),
            kill_reason: None,
            killed_at: None,
            last_cwd: None,
            last_command: None,
            last_exit_code: None,
            created_at: super::unix_seconds_now(),
            owner_pid: Some(std::process::id() as i64),
            owner_boot_time: None,
        };
        let _ = h.db.insert_snapshot(&row);
        term.attach_persistence(&h.db, sid);
    }));
}
