//! C ABI for the Rust-owned block list. Lifetime contract:
//!
//! - `bt_term_block_count` is cheap and always safe; reflects the state
//!   left by the most recent `bt_term_feed`.
//! - `bt_term_block_at` returns by-value POD plus borrow-out pointers
//!   into `BtTerm::block_string_scratch`. Those string pointers stay
//!   valid only until the next call to ANY `bt_term_*` that mutates the
//!   terminal OR the next `bt_term_block_at` call (the scratch is
//!   reused). Swift must copy out strings inside the same call.
use crate::blocks::BLOCK_END_LINE_RUNNING;
use crate::cli_agent::CliAgent;
use crate::ffi::BtTerm;
use std::os::raw::c_char;
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
    /// CLI agent tag — `0` = none / unrecognised, otherwise one of the
    /// `BT_CLI_AGENT_*` constants below. Stable across releases; new
    /// agents append. Identifying a known agent lets the host paint a
    /// brand icon next to the block header; does NOT change layout.
    pub cli_agent: u8,
    pub _pad3: [u8; 2],
    /// Live body height in grid rows. For sealed blocks this equals
    /// `end_line - start_line`. For running blocks this is the block
    /// grid's current `used_rows()` — the bottom-most non-blank visible
    /// row + 1 (plus any history). The host should use this for the
    /// block's body extent so a streaming TUI grows the block in real
    /// time without pre-allocating the full PTY screen height.
    pub body_rows: u32,
}

pub const BT_BLOCK_END_LINE_RUNNING: i32 = BLOCK_END_LINE_RUNNING;

pub const BT_CLI_AGENT_NONE: u8 = 0;
pub const BT_CLI_AGENT_CLAUDE: u8 = 1;
pub const BT_CLI_AGENT_GEMINI: u8 = 2;
pub const BT_CLI_AGENT_CODEX: u8 = 3;
pub const BT_CLI_AGENT_AMP: u8 = 4;
pub const BT_CLI_AGENT_DROID: u8 = 5;
pub const BT_CLI_AGENT_OPENCODE: u8 = 6;
pub const BT_CLI_AGENT_COPILOT: u8 = 7;
pub const BT_CLI_AGENT_PI: u8 = 8;
pub const BT_CLI_AGENT_AUGGIE: u8 = 9;
pub const BT_CLI_AGENT_CURSOR_CLI: u8 = 10;
pub const BT_CLI_AGENT_GOOSE: u8 = 11;
pub const BT_CLI_AGENT_HERMES: u8 = 12;
pub const BT_CLI_AGENT_VIBE: u8 = 13;

/// # Safety
/// `h` must be a valid `BtTerm *` returned by `bt_term_new`.
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
        cli_agent_tag,
        body_rows,
    ) = {
        let Some(block) = term.inner_ref().block_at(idx) else {
            return -1;
        };
        // For running blocks ask the live grid for its used_rows so the
        // host body extent tracks the printed content in real time. For
        // sealed blocks the snapshot's recorded `end_line - start_line`
        // is the source of truth (matches the frozen_snapshot's row
        // count exactly).
        let body_rows = if block.is_running {
            block
                .grid
                .as_ref()
                .map(|g| u32::from(g.used_rows()))
                .unwrap_or(1)
        } else {
            // Sealed blocks: honour the captured row count exactly.
            // Commands like `cd /tmp` produce zero output and should
            // collapse their body, not leave a blank gap below the
            // header. The (end - start) span is non-negative.
            (block.end_line - block.start_line).max(0) as u32
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
            block
                .cli_agent
                .map(|a| a.ffi_tag())
                .unwrap_or(BT_CLI_AGENT_NONE),
            body_rows,
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
        cli_agent: cli_agent_tag,
        _pad3: [0; 2],
        body_rows,
    };
    0
}

/// Return the human-readable display name for a CLI agent identified by
/// its `BT_CLI_AGENT_*` tag. Returns a static C string, or NULL when the
/// tag is `BT_CLI_AGENT_NONE` (0) or unknown (forward-compat).
///
/// The returned pointer lives in the binary's `.rodata` and must NOT be
/// freed by the caller.
pub extern "C" fn bt_cli_agent_display_name(tag: u8) -> *const c_char {
    match tag {
        BT_CLI_AGENT_NONE => std::ptr::null(),
        BT_CLI_AGENT_CLAUDE => CliAgent::Claude.display_name().as_ptr().cast(),
        BT_CLI_AGENT_GEMINI => CliAgent::Gemini.display_name().as_ptr().cast(),
        BT_CLI_AGENT_CODEX => CliAgent::Codex.display_name().as_ptr().cast(),
        BT_CLI_AGENT_AMP => CliAgent::Amp.display_name().as_ptr().cast(),
        BT_CLI_AGENT_DROID => CliAgent::Droid.display_name().as_ptr().cast(),
        BT_CLI_AGENT_OPENCODE => CliAgent::OpenCode.display_name().as_ptr().cast(),
        BT_CLI_AGENT_COPILOT => CliAgent::Copilot.display_name().as_ptr().cast(),
        BT_CLI_AGENT_PI => CliAgent::Pi.display_name().as_ptr().cast(),
        BT_CLI_AGENT_AUGGIE => CliAgent::Auggie.display_name().as_ptr().cast(),
        BT_CLI_AGENT_CURSOR_CLI => CliAgent::CursorCli.display_name().as_ptr().cast(),
        BT_CLI_AGENT_GOOSE => CliAgent::Goose.display_name().as_ptr().cast(),
        BT_CLI_AGENT_HERMES => CliAgent::Hermes.display_name().as_ptr().cast(),
        BT_CLI_AGENT_VIBE => CliAgent::Vibe.display_name().as_ptr().cast(),
        _ => std::ptr::null(),
    }
}

/// Return the asset-catalog icon name for a CLI agent identified by its
/// `BT_CLI_AGENT_*` tag. Returns a static C string, or NULL when the agent
/// has no dedicated icon (caller should fall back to a generic glyph),
/// the tag is `BT_CLI_AGENT_NONE` (0), or unknown.
///
/// The returned pointer lives in the binary's `.rodata` and must NOT be
/// freed by the caller.
pub extern "C" fn bt_cli_agent_icon_name(tag: u8) -> *const c_char {
    match tag {
        BT_CLI_AGENT_CLAUDE => CliAgent::Claude
            .icon_name()
            .map_or(std::ptr::null(), |s| s.as_ptr().cast()),
        BT_CLI_AGENT_CODEX => CliAgent::Codex
            .icon_name()
            .map_or(std::ptr::null(), |s| s.as_ptr().cast()),
        _ => std::ptr::null(),
    }
}
