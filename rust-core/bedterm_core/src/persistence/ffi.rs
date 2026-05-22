//! C ABI for the persistence module. cbindgen emits these into the
//! generated `bedterm_core.h`. Conventions: pointers may be null;
//! all string pointers are NUL-terminated UTF-8; functions never
//! panic across the FFI boundary (wrapped in `catch_unwind`).

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use super::Database;
use crate::term::Terminal;

// ── C-ABI snapshot list types ─────────────────────────────────────────────────

/// A single snapshot row as seen from Swift / C.
#[repr(C)]
pub struct CSnapshot {
    pub id: *const c_char,
    pub host_id: *const c_char,
    /// -1 if no kill_reason was recorded.
    pub kill_reason: i32,
    /// 0.0 if no killed_at was recorded.
    pub killed_at: f64,
    /// null if no last_cwd was recorded.
    pub last_cwd: *const c_char,
    /// null if no last_command was recorded.
    pub last_command: *const c_char,
    /// `i32::MIN` if no exit code was recorded.
    pub last_exit_code: i32,
    pub block_count: i64,
}

/// Heap-allocated list returned by `bedterm_persistence_list`.
/// Free with `bedterm_persistence_free_list`.
#[repr(C)]
pub struct CSnapshotList {
    pub items: *const CSnapshot,
    pub count: usize,
    pub(crate) _owned: *mut OwnedListBacking,
}

/// Backing storage that keeps the `CString` allocations alive for as long as
/// the `CSnapshotList` is live.
pub(crate) struct OwnedListBacking {
    pub items: Vec<CSnapshot>,
    // `strings` is never read after construction; it exists solely to keep
    // the heap allocations alive so that the raw pointers in `items` remain
    // valid.
    #[allow(dead_code)]
    pub strings: Vec<CString>,
}

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

// ── record_kill / list / free_list / discard ──────────────────────────────────

/// Record that a session was killed.
///
/// - `reason`: a `KillReason` discriminant (0–4).
/// - `last_cwd`, `last_command`: NUL-terminated UTF-8 or null.
/// - `last_exit_code` / `has_exit_code`: use `has_exit_code != 0` to pass a
///   real exit code; `i32::MIN` is a legal exit code so a sentinel is not safe.
///
/// # Safety
/// All non-null pointer arguments must point to valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn bedterm_persistence_record_kill(
    handle: *mut PersistenceHandle,
    snapshot_id: *const c_char,
    reason: i32,
    last_cwd: *const c_char,
    last_command: *const c_char,
    last_exit_code: i32,
    has_exit_code: i32,
) {
    if handle.is_null() || snapshot_id.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let h = unsafe { &mut *handle };
        let sid = unsafe { CStr::from_ptr(snapshot_id) }
            .to_str()
            .unwrap_or("");
        let kill_reason = crate::persistence::KillReason::from_i32(reason)
            .unwrap_or(crate::persistence::KillReason::UserKilled);
        let cwd = if last_cwd.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(last_cwd) }.to_str().ok()
        };
        let cmd = if last_command.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(last_command) }.to_str().ok()
        };
        let exit = if has_exit_code != 0 {
            Some(last_exit_code)
        } else {
            None
        };
        let _ = h.db.record_kill(sid, kill_reason, cwd, cmd, exit);
    }));
}

/// List all killed snapshots for a host, ordered newest-first.
///
/// Returns a heap-allocated `CSnapshotList` that must be freed with
/// `bedterm_persistence_free_list`. Returns null on error.
///
/// # Safety
/// `handle` and `host_id` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn bedterm_persistence_list(
    handle: *mut PersistenceHandle,
    host_id: *const c_char,
) -> *mut CSnapshotList {
    if handle.is_null() || host_id.is_null() {
        return std::ptr::null_mut();
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let h = unsafe { &mut *handle };
        let hid = unsafe { CStr::from_ptr(host_id) }.to_str().ok()?;
        let rows = h.db.list_snapshots(hid).ok()?;

        // Pass 1 — build every CString and push into `strings`.
        // CString heap-allocates its content; `as_ptr` returns a pointer to
        // that heap content, which stays valid even if `strings` reallocates.
        let mut strings: Vec<CString> = Vec::new();
        for r in &rows {
            strings.push(CString::new(r.id.clone()).ok()?);
            strings.push(CString::new(r.host_id.clone()).ok()?);
            if let Some(ref s) = r.last_cwd {
                strings.push(CString::new(s.clone()).ok()?);
            }
            if let Some(ref s) = r.last_command {
                strings.push(CString::new(s.clone()).ok()?);
            }
        }

        // Pass 2 — build CSnapshot items by walking `strings` in the same
        // order they were pushed.
        let mut sidx = 0usize;
        let mut items: Vec<CSnapshot> = Vec::with_capacity(rows.len());
        for r in &rows {
            let id_ptr = strings[sidx].as_ptr();
            sidx += 1;
            let host_ptr = strings[sidx].as_ptr();
            sidx += 1;
            let cwd_ptr = if r.last_cwd.is_some() {
                let p = strings[sidx].as_ptr();
                sidx += 1;
                p
            } else {
                std::ptr::null()
            };
            let cmd_ptr = if r.last_command.is_some() {
                let p = strings[sidx].as_ptr();
                sidx += 1;
                p
            } else {
                std::ptr::null()
            };

            let block_count: i64 =
                h.db.conn
                    .query_row(
                        "SELECT COUNT(*) FROM blocks WHERE snapshot_id = ?1",
                        rusqlite::params![r.id],
                        |x| x.get(0),
                    )
                    .unwrap_or(0);

            items.push(CSnapshot {
                id: id_ptr,
                host_id: host_ptr,
                kill_reason: r.kill_reason.map(|k| k as i32).unwrap_or(-1),
                killed_at: r.killed_at.unwrap_or(0.0),
                last_cwd: cwd_ptr,
                last_command: cmd_ptr,
                last_exit_code: r.last_exit_code.unwrap_or(i32::MIN),
                block_count,
            });
        }

        let owned = Box::new(OwnedListBacking { items, strings });
        let items_ptr = owned.items.as_ptr();
        let count = owned.items.len();
        let owned_raw = Box::into_raw(owned);
        Some(Box::into_raw(Box::new(CSnapshotList {
            items: items_ptr,
            count,
            _owned: owned_raw,
        })))
    }));
    result.ok().flatten().unwrap_or(std::ptr::null_mut())
}

/// Free a list previously returned by `bedterm_persistence_list`.
///
/// # Safety
/// `list` must have been returned by `bedterm_persistence_list` and not yet
/// freed.
#[no_mangle]
pub unsafe extern "C" fn bedterm_persistence_free_list(list: *mut CSnapshotList) {
    if list.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let l = unsafe { Box::from_raw(list) };
        if !l._owned.is_null() {
            drop(unsafe { Box::from_raw(l._owned) });
        }
    }));
}

/// Open a replay terminal pre-loaded with the stored blocks for `snapshot_id`.
///
/// Returns a newly-allocated `BtTerm` that has been fed all stored block bytes
/// for the given snapshot. The terminal has no PTY backing and no persistence
/// sink — it is read-only and renders the session history via the normal Metal
/// renderer. Free the returned pointer with `bt_term_free`.
///
/// Returns null if `snapshot_id` is unknown, has no blocks, or an error occurs.
///
/// # Safety
/// `handle` and `snapshot_id` must be valid non-null pointers.
/// `snapshot_id` must be a NUL-terminated UTF-8 C string.
/// The returned `BtTerm *` must be freed with `bt_term_free` (existing FFI).
#[no_mangle]
pub unsafe extern "C" fn bedterm_persistence_open_replay(
    handle: *mut PersistenceHandle,
    snapshot_id: *const c_char,
) -> *mut crate::ffi::BtTerm {
    if handle.is_null() || snapshot_id.is_null() {
        return std::ptr::null_mut();
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let h = unsafe { &mut *handle };
        let sid = unsafe { CStr::from_ptr(snapshot_id) }.to_str().ok()?;
        let rows = h.db.load_blocks(sid).ok()?;
        if rows.is_empty() {
            return None;
        }

        // Allocate a replay-only BtTerm.
        let term_ptr = crate::ffi::bt_term_new_replay(80, 24);
        if term_ptr.is_null() {
            return None;
        }
        let term = unsafe { &mut *term_ptr };

        // Re-wrap each stored block in minimal DCS frames so the DCS sniffer
        // in `Terminal::feed` can advance its state machine and seal blocks.
        // `stylized_command` is always empty in the DCS protocol (command text
        // arrives via the Preexec JSON payload). `stylized_output` holds raw
        // output bytes without DCS framing (captured between Preexec and
        // CommandFinished).
        for row in &rows {
            let mut block_bytes = Vec::new();

            // Build DCS JSON strings inline to avoid a serde_json dependency.
            let cwd = row.cwd.as_deref().unwrap_or("/");
            let precmd = format!(r#"{{"hook":"Precmd","value":{{"pwd":"{cwd}"}}}}"#);
            block_bytes.extend_from_slice(&dcs_frame(precmd.as_bytes()));

            let cmd = &row.command;
            let preexec = format!(r#"{{"hook":"Preexec","value":{{"command":"{cmd}"}}}}"#);
            block_bytes.extend_from_slice(&dcs_frame(preexec.as_bytes()));

            // Raw output bytes — no framing (captured verbatim).
            block_bytes.extend_from_slice(&row.stylized_output);

            let exit = row.exit_code.unwrap_or(0);
            let finished =
                format!(r#"{{"hook":"CommandFinished","value":{{"exit_code":{exit}}}}}"#);
            block_bytes.extend_from_slice(&dcs_frame(finished.as_bytes()));

            term.feed_bytes(&block_bytes);
        }

        Some(term_ptr)
    }));
    result.ok().flatten().unwrap_or(std::ptr::null_mut())
}

/// Build a DCS JSON frame: `ESC P $ d <hex(json)> 0x9C`.
fn dcs_frame(json: &[u8]) -> Vec<u8> {
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(json.len() * 2);
    for b in json {
        write!(hex, "{b:02x}").unwrap();
    }
    let mut v = vec![0x1B, b'P', b'$', b'd'];
    v.extend_from_slice(hex.as_bytes());
    v.push(0x9C);
    v
}

/// Delete a snapshot (and its blocks) from the database.
///
/// # Safety
/// `handle` and `snapshot_id` must be valid non-null pointers; `snapshot_id`
/// must be a NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bedterm_persistence_discard(
    handle: *mut PersistenceHandle,
    snapshot_id: *const c_char,
) {
    if handle.is_null() || snapshot_id.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let h = unsafe { &mut *handle };
        let sid = unsafe { CStr::from_ptr(snapshot_id) }
            .to_str()
            .unwrap_or("");
        let _ = h.db.discard_snapshot(sid);
    }));
}
