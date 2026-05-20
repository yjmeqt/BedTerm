//! C ABI for the Rust-owned block list. Lifetime contract:
//!
//! - `bt_term_block_count` is cheap and always safe; reflects the state
//!   left by the most recent `bt_term_feed`.
//! - `bt_term_block_at` returns by-value POD plus borrow-out pointers
//!   into `BtTerm::block_string_scratch`. Those string pointers stay
//!   valid only until the next call to ANY `bt_term_*` that mutates the
//!   terminal OR the next `bt_term_block_at` call (the scratch is
//!   reused). Swift must copy out strings inside the same call.
//! - `bt_term_block_snapshot` exposes the frozen body of a sealed block.
//!   The cell pointer follows the same invalidation rules as
//!   `bt_term_snapshot`.

use crate::blocks::BLOCK_END_LINE_RUNNING;
use crate::ffi::{BtSnapshotView, BtTerm};
use std::os::raw::c_int;

#[repr(C)]
pub struct BtBlockView {
    pub id: u64,
    pub start_line: i32,
    /// `BT_BLOCK_END_LINE_RUNNING` while the block is running.
    pub end_line: i32,
    /// 1 if running, 0 otherwise.
    pub is_running: u8,
    pub has_exit_code: u8,
    pub _pad: [u8; 2],
    pub exit_code: i32,
    /// 0 when the shell didn't ship `dur=`.
    pub duration_ms: u64,
    pub has_duration: u8,
    pub _pad2: [u8; 7],
    /// UTF-8 bytes for command. `null` + len=0 when empty.
    pub command: *const u8,
    pub command_len: usize,
    /// UTF-8 bytes for working directory. `null` + len=0 when missing.
    pub cwd: *const u8,
    pub cwd_len: usize,
    /// UTF-8 bytes for git branch (short name or short SHA when
    /// detached). `null` + len=0 when cwd is outside a repo or the
    /// shell-integration didn't ship the field.
    pub git_branch: *const u8,
    pub git_branch_len: usize,
    /// 1 if `frozen_snapshot` is available (sealed block), 0 otherwise.
    pub has_frozen_snapshot: u8,
    pub _pad3: [u8; 7],
}

pub const BT_BLOCK_END_LINE_RUNNING: i32 = BLOCK_END_LINE_RUNNING;

/// # Safety
/// `h` must be a valid `BtTerm *` returned by `bt_term_new`.
#[no_mangle]
pub unsafe extern "C" fn bt_term_block_count(h: *const BtTerm) -> usize {
    if h.is_null() {
        return 0;
    }
    (*h).inner_ref().block_count()
}

/// # Safety
/// `h` must be a valid `BtTerm *`; `out` must point to a writable
/// `BtBlockView`. String pointers in `*out` are invalidated by the next
/// call as described in the module-level docs.
#[no_mangle]
pub unsafe extern "C" fn bt_term_block_at(
    h: *mut BtTerm,
    idx: usize,
    out: *mut BtBlockView,
) -> c_int {
    if h.is_null() || out.is_null() {
        return -1;
    }
    let term = &mut *h;
    // Copy out everything we need from the borrowed Block FIRST so the
    // borrow ends before we touch the scratch buffer.
    let (
        id,
        start_line,
        end_line,
        is_running,
        exit_code,
        duration_ms,
        has_frozen_snapshot,
        command,
        working_directory,
        git_branch,
    ) = {
        let Some(block) = term.inner_ref().block_at(idx) else {
            return -1;
        };
        (
            block.id,
            block.start_line,
            block.end_line,
            block.is_running,
            block.exit_code,
            block.duration_ms,
            block.frozen_snapshot.is_some(),
            block.command.clone(),
            block.working_directory.clone(),
            block.git_branch.clone(),
        )
    };

    // Pack strings into the per-BtTerm scratch buffer; record offsets so
    // we can compute pointers after all writes (Vec may reallocate
    // mid-extend).
    let scratch = term.block_string_scratch_mut();
    scratch.clear();
    let command_offset = scratch.len();
    scratch.extend_from_slice(command.as_bytes());
    let command_len = command.len();
    let (cwd_offset, cwd_len) = if let Some(cwd) = working_directory.as_ref() {
        let start = scratch.len();
        scratch.extend_from_slice(cwd.as_bytes());
        (start, cwd.len())
    } else {
        (0, 0)
    };
    let (branch_offset, branch_len) = if let Some(branch) = git_branch.as_ref() {
        let start = scratch.len();
        scratch.extend_from_slice(branch.as_bytes());
        (start, branch.len())
    } else {
        (0, 0)
    };
    let base = scratch.as_ptr();
    let command_ptr = if command_len > 0 {
        base.add(command_offset)
    } else {
        std::ptr::null()
    };
    let cwd_ptr = if cwd_len > 0 {
        base.add(cwd_offset)
    } else {
        std::ptr::null()
    };
    let branch_ptr = if branch_len > 0 {
        base.add(branch_offset)
    } else {
        std::ptr::null()
    };

    *out = BtBlockView {
        id,
        start_line,
        end_line,
        is_running: if is_running { 1 } else { 0 },
        has_exit_code: if exit_code.is_some() { 1 } else { 0 },
        _pad: [0; 2],
        exit_code: exit_code.unwrap_or(0),
        duration_ms: duration_ms.unwrap_or(0),
        has_duration: if duration_ms.is_some() { 1 } else { 0 },
        _pad2: [0; 7],
        command: command_ptr,
        command_len,
        cwd: cwd_ptr,
        cwd_len,
        git_branch: branch_ptr,
        git_branch_len: branch_len,
        has_frozen_snapshot: if has_frozen_snapshot { 1 } else { 0 },
        _pad3: [0; 7],
    };
    0
}

/// Fetch the frozen body of a sealed block. Returns -1 if the block is
/// still running, missing, or the index is out of bounds. The cell
/// pointer in `*out` follows the same invalidation rules as
/// `bt_term_snapshot`.
///
/// # Safety
/// `h` valid; `out` writable.
#[no_mangle]
pub unsafe extern "C" fn bt_term_block_snapshot(
    h: *mut BtTerm,
    idx: usize,
    out: *mut BtSnapshotView,
) -> c_int {
    if h.is_null() || out.is_null() {
        return -1;
    }
    let term = &mut *h;
    // Clone the snapshot out so the immutable borrow on `term` ends
    // before we cache it.
    let Some(snap) = term
        .inner_ref()
        .block_at(idx)
        .and_then(|b| b.frozen_snapshot.clone())
    else {
        return -1;
    };
    *out = BtSnapshotView {
        cols: snap.cols,
        rows: snap.rows,
        cursor_col: snap.cursor_col,
        cursor_row: snap.cursor_row,
        display_offset: snap.display_offset,
        cells: snap.cells.as_ptr(),
        cell_count: snap.cells.len(),
    };
    term.set_block_snapshot_cached(snap);
    0
}

/// # Safety
/// `h` valid.
#[no_mangle]
pub unsafe extern "C" fn bt_term_block_snapshot_release(h: *mut BtTerm) {
    if h.is_null() {
        return;
    }
    (*h).clear_block_snapshot_cached();
}
