//! Warp-style command-block state machine. Consumes the DCS event stream
//! emitted by `dcs::DcsSniffer` and produces a list of `Block`s: row
//! ranges into the shared terminal grid, sealed with a frozen `GridSnapshot`
//! at command-end so they survive scrollback eviction.
//!
//! Wall-clock duration is computed on the Rust side as the delta between
//! `Preexec` and `CommandFinished` boundaries — the shell doesn't ship a
//! `duration_ms` field (and neither does Warp's wire format).

use crate::block_grid::BlockGrid;
use crate::cli_agent::CliAgent;
use crate::dcs::DcsEvent;
use crate::snapshot::RawGridSnapshot;
use crate::term::Palette;
use std::time::Instant;

/// Stable monotonic block id, assigned at open. Never reused inside a single
/// `BlockStore`; survives `reset()` by continuing to count up so Swift `@Observable`
/// diffs stay clean across reconnects.
pub type BlockId = u64;

/// Maximum number of newline characters stored in `stylized_command` or
/// `stylized_output`. Bytes beyond this limit are silently dropped so a
/// long-running command with voluminous output doesn't grow the in-memory
/// capture without bound.
pub const MAX_BLOCK_OUTPUT_LINES: u32 = 5000;

/// Sentinel for a still-running block's `end_line`. Picked outside the legal
/// `i32` grid-line range alacritty produces. Public so the FFI layer can
/// surface it to Swift.
pub const BLOCK_END_LINE_RUNNING: i32 = i32::MIN;

#[derive(Debug)]
pub struct Block {
    pub id: BlockId,
    pub command: String,
    pub start_line: i32,
    /// `BLOCK_END_LINE_RUNNING` while the block is still running.
    pub end_line: i32,
    /// `None` while running. Stored palette-agnostic so a system
    /// light↔dark flip after seal re-resolves the cells via the live
    /// `Palette` rather than locking them to the freeze-time RGBA.
    pub frozen_snapshot: Option<RawGridSnapshot>,
    /// `None` if the block sealed without a `CommandFinished` (Ctrl-C path,
    /// partial integration).
    pub exit_code: Option<i32>,
    /// Wall-clock command duration. `None` if no matching `Preexec` was
    /// observed before the seal.
    pub duration_ms: Option<u64>,
    /// Working directory captured at the opening `Precmd`. `None` if the
    /// shell didn't ship `pwd`.
    pub working_directory: Option<String>,
    /// Git branch (short name, or short SHA when detached) captured at
    /// the opening `Precmd`. `None` when cwd is outside a repo or the
    /// remote `git` is missing.
    pub git_branch: Option<String>,
    /// CLI agent identified at `Preexec` from the command line — e.g.
    /// `claude`, `codex`, `gemini`. `None` for unrecognised commands.
    /// Mirrors Warp's `CLIAgent::detect`; informs branding only, not
    /// layout decisions.
    pub cli_agent: Option<CliAgent>,
    /// Per-block private VTE + grid. Set by `Terminal::feed` when the
    /// `Preexec` boundary is processed (the command starts producing
    /// output); cleared at `CommandFinished` after the final state
    /// has been frozen into `frozen_snapshot`. Routing of command
    /// output bytes targets this grid so cursor positioning,
    /// erase-line, and redraws inside a TUI stay scoped to the
    /// block's body — mirrors Warp's `output_grid: BlockGrid` per
    /// block. `None` for sealed blocks (already snapshot) and for
    /// open-but-not-yet-Preexec blocks (no command running yet).
    pub grid: Option<BlockGrid>,
    pub is_running: bool,
    /// Raw bytes received between the `Preexec` event (command starts
    /// running) and the `CommandFinished` event, captured verbatim with
    /// all ANSI/SGR escape sequences intact so replay can reconstruct
    /// the exact rendered output. Capped at `MAX_BLOCK_OUTPUT_LINES`
    /// newlines; bytes beyond that limit are silently dropped.
    ///
    /// Note: this DCS-based protocol has no separate "command echo"
    /// phase (unlike iTerm2's OSC 133;B/C split), so `stylized_command`
    /// is always empty in the current implementation. It is reserved for
    /// future protocol extensions.
    pub stylized_command: Vec<u8>,
    /// Bytes received between `Preexec` and `CommandFinished`. Same cap
    /// as `stylized_command`. Populated in parallel with `block.grid`
    /// feeding — both see the same raw byte slices.
    pub stylized_output: Vec<u8>,
    pub stylized_command_lines: u32,
    pub stylized_output_lines: u32,
}

impl Block {
    /// Append `bytes` to `stylized_output`, stopping once
    /// `MAX_BLOCK_OUTPUT_LINES` newlines have been captured.
    pub fn append_output_bytes(&mut self, bytes: &[u8]) {
        Self::append_capped(
            &mut self.stylized_output,
            &mut self.stylized_output_lines,
            bytes,
        );
    }

    /// Append `bytes` to `stylized_command`. Reserved for future use —
    /// the current DCS protocol does not produce command-phase bytes.
    pub fn append_command_bytes(&mut self, bytes: &[u8]) {
        Self::append_capped(
            &mut self.stylized_command,
            &mut self.stylized_command_lines,
            bytes,
        );
    }

    fn append_capped(buf: &mut Vec<u8>, lines: &mut u32, bytes: &[u8]) {
        for &b in bytes {
            if *lines >= MAX_BLOCK_OUTPUT_LINES {
                return;
            }
            buf.push(b);
            if b == b'\n' {
                *lines += 1;
            }
        }
    }
}

pub struct BlockStore {
    blocks: Vec<Block>,
    next_id: BlockId,
    open_index: Option<usize>,
    /// Recorded on `Preexec`; consumed on `CommandFinished` to compute
    /// `duration_ms`. Cleared on `reset()`.
    command_start: Option<Instant>,
}

impl Default for BlockStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockStore {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            next_id: 1,
            open_index: None,
            command_start: None,
        }
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<&Block> {
        self.blocks.get(idx)
    }

    pub fn reset(&mut self) {
        self.blocks.clear();
        self.open_index = None;
        self.command_start = None;
        // `next_id` deliberately NOT reset — see doc on BlockId.
    }

    /// Apply one DCS event.
    ///
    /// - `current_line` is the cursor's current grid line at the moment the
    ///   event arrives (used as the row boundary).
    /// - `now` is the wall-clock instant the event arrived.
    /// - `cols` / `rows` are the live PTY screen size; used to size the
    ///   per-block grid attached on `Preexec`.
    /// - `palette` resolves named colours when freezing the block's grid
    ///   into `frozen_snapshot` at `CommandFinished`.
    ///
    /// Block lifetime:
    ///   - **`Precmd`**: open a fresh block (no grid yet — there's no
    ///     command running). Seal any previously-open block first (Ctrl-C
    ///     path).
    ///   - **`Preexec`**: fill the block's command + cli_agent + advance
    ///     `start_line` past the prompt echo. **Attach a private
    ///     `BlockGrid`** so subsequent output bytes can be routed to it
    ///     instead of the global terminal.
    ///   - **`CommandFinished`**: freeze the block's grid into
    ///     `frozen_snapshot`, then drop the grid.
    pub fn apply(
        &mut self,
        event: &DcsEvent,
        current_line: i32,
        now: Instant,
        cols: u16,
        rows: u16,
        palette: &Palette,
    ) {
        match event {
            DcsEvent::Precmd { pwd, git_branch } => {
                // A bare Precmd means "about to draw a prompt". If a block
                // is still open it never got a CommandFinished (Ctrl-C,
                // partial integration); seal it with no exit + no duration.
                self.seal_open(current_line, None, palette);
                self.open_new(current_line, pwd.clone(), git_branch.clone());
            }
            DcsEvent::Preexec { command } => {
                if self.open_index.is_none() {
                    self.open_new(current_line, None, None);
                }
                if let Some(idx) = self.open_index {
                    self.blocks[idx].command = command.clone();
                    self.blocks[idx].cli_agent = CliAgent::detect(command);
                    // Move `start_line` forward to the row Preexec fires
                    // on — that's the line just after the prompt + the
                    // echoed command, i.e. where the command's output
                    // begins. Warp displays only the output in a block's
                    // body; the prompt/command live in the header. By
                    // advancing the body's row range we hide the echoed
                    // prompt line from the block snapshot.
                    self.blocks[idx].start_line = current_line;
                    // Per-block grid: attach a private VTE + grid so the
                    // command's bytes get routed here instead of the
                    // global terminal. Idempotent — only attach once per
                    // block lifecycle.
                    if self.blocks[idx].grid.is_none() {
                        self.blocks[idx].grid = Some(BlockGrid::new(cols, rows));
                    }
                }
                self.command_start = Some(now);
            }
            DcsEvent::CommandFinished { exit_code } => {
                let duration_ms = self
                    .command_start
                    .take()
                    .map(|start| now.saturating_duration_since(start).as_millis() as u64);
                self.seal_with(current_line, Some(*exit_code), duration_ms, palette);
            }
        }
    }

    /// Mutable access to the currently-open block (the running one).
    /// `None` after `CommandFinished` until the next `Precmd`. Used by
    /// `Terminal::feed` to route command output bytes into the open
    /// block's private grid.
    pub fn open_block_mut(&mut self) -> Option<&mut Block> {
        let idx = self.open_index?;
        self.blocks.get_mut(idx)
    }

    /// Whether there is currently an open (running) block. Used by
    /// `Terminal::feed` to detect whether a `Precmd` event will seal an
    /// existing block (true) or is arriving fresh (false).
    pub fn has_open_block(&self) -> bool {
        self.open_index.is_some()
    }

    /// Resize every still-attached `BlockGrid` to the new PTY
    /// geometry — called by `Terminal::resize` so a running command
    /// inside a TUI doesn't get scrambled when the iOS view bounds
    /// change. The global terminal resize is handled separately by
    /// the caller.
    pub fn resize_grids(&mut self, cols: u16, rows: u16) {
        for block in &mut self.blocks {
            if let Some(grid) = block.grid.as_mut() {
                grid.resize(cols, rows);
            }
        }
    }

    /// Return a reference to the most-recently-sealed (non-running) block.
    pub fn last_finalized(&self) -> Option<&Block> {
        self.blocks.iter().rev().find(|b| !b.is_running)
    }

    fn open_new(&mut self, current_line: i32, pwd: Option<String>, git_branch: Option<String>) {
        let id = self.next_id;
        self.next_id += 1;
        self.blocks.push(Block {
            id,
            command: String::new(),
            start_line: current_line,
            end_line: BLOCK_END_LINE_RUNNING,
            frozen_snapshot: None,
            exit_code: None,
            duration_ms: None,
            working_directory: pwd,
            git_branch,
            cli_agent: None,
            grid: None,
            is_running: true,
            stylized_command: Vec::new(),
            stylized_output: Vec::new(),
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        });
        self.open_index = Some(self.blocks.len() - 1);
    }

    /// Seal the currently-open block (if any) without exit/duration —
    /// used when a Precmd arrives over an already-running block.
    fn seal_open(&mut self, current_line: i32, exit_code: Option<i32>, palette: &Palette) {
        self.seal_with(current_line, exit_code, None, palette);
    }

    fn seal_with(
        &mut self,
        current_line: i32,
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
        palette: &Palette,
    ) {
        let Some(idx) = self.open_index else { return };
        if !self.blocks[idx].is_running {
            return;
        }
        // With per-block VTE routing the global cursor barely moves
        // during a command (bytes flow into `block.grid` instead), so
        // `current_line` would give a body 0–1 rows tall. Ask the
        // grid for its full used row count (history scrollback +
        // currently-occupied visible rows). A `seq 80` into a 29-row
        // grid returns 80; a `ls` into the same grid returns
        // (e.g.) 5. Must match the row count produced by
        // `BlockGrid::snapshot()` so the renderer's body extent and
        // the host's body height agree.
        // Palette is no longer consulted at seal — we freeze the raw
        // colour intent so light↔dark flips after seal can re-resolve.
        let _ = palette;
        let (snap, used_rows) = match self.blocks[idx].grid.as_ref() {
            Some(g) => (Some(g.snapshot_raw()), g.used_rows() as i32),
            None => (
                None,
                current_line.saturating_add(1) - self.blocks[idx].start_line,
            ),
        };
        let block = &mut self.blocks[idx];
        block.is_running = false;
        block.end_line = block.start_line.saturating_add(used_rows.max(1));
        block.frozen_snapshot = snap;
        block.grid = None;
        block.exit_code = exit_code;
        block.duration_ms = duration_ms;
        self.open_index = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Test helpers — synthesise a fresh BlockStore with a known
    /// PTY geometry + default palette. Callers apply DCS events via
    /// `apply_event` which forwards to `BlockStore::apply` with the
    /// same fixed cols/rows/palette so each test stays terse.
    const TEST_COLS: u16 = 20;
    const TEST_ROWS: u16 = 5;

    fn apply_event(store: &mut BlockStore, event: &DcsEvent, current_line: i32, now: Instant) {
        store.apply(
            event,
            current_line,
            now,
            TEST_COLS,
            TEST_ROWS,
            &Palette::default(),
        );
    }

    fn precmd(pwd: Option<&str>) -> DcsEvent {
        DcsEvent::Precmd {
            pwd: pwd.map(str::to_owned),
            git_branch: None,
        }
    }

    fn preexec(cmd: &str) -> DcsEvent {
        DcsEvent::Preexec {
            command: cmd.to_owned(),
        }
    }

    fn command_finished(exit: i32) -> DcsEvent {
        DcsEvent::CommandFinished { exit_code: exit }
    }

    #[test]
    fn empty_on_init() {
        let s = BlockStore::new();
        assert!(s.is_empty());
    }

    #[test]
    fn first_precmd_opens_running_block_with_pwd() {
        let mut s = BlockStore::new();
        apply_event(&mut s, &precmd(Some("/home/alice")), 5, Instant::now());
        assert_eq!(s.len(), 1);
        let b = s.get(0).unwrap();
        assert_eq!(b.start_line, 5);
        assert_eq!(b.end_line, BLOCK_END_LINE_RUNNING);
        assert!(b.is_running);
        assert_eq!(b.working_directory.as_deref(), Some("/home/alice"));
        assert!(b.frozen_snapshot.is_none());
    }

    #[test]
    fn preexec_fills_command_and_starts_clock() {
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        apply_event(&mut s, &precmd(None), 0, t0);
        apply_event(&mut s, &preexec("ls"), 0, t0);
        assert_eq!(s.get(0).unwrap().command, "ls");
    }

    #[test]
    fn command_finished_seals_and_records_duration() {
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        apply_event(&mut s, &precmd(Some("/tmp")), 3, t0);
        apply_event(&mut s, &preexec("ls"), 3, t0);
        apply_event(
            &mut s,
            &command_finished(0),
            7,
            t0 + Duration::from_millis(1234),
        );
        let first = s.get(0).unwrap();
        assert!(!first.is_running);
        // Per-block grid was attached at Preexec with cursor at row 0;
        // no bytes were fed before CommandFinished so `used_rows == 1`
        // and `end_line == start_line + 1`. (The CommandFinished
        // `current_line` argument is ignored when a grid is present —
        // the grid's own cursor row is authoritative.)
        assert_eq!(first.end_line, first.start_line + 1);
        assert_eq!(first.exit_code, Some(0));
        assert_eq!(first.duration_ms, Some(1234));
        assert!(first.frozen_snapshot.is_some());
    }

    #[test]
    fn nonzero_exit_carried() {
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        apply_event(&mut s, &precmd(None), 0, t0);
        apply_event(&mut s, &preexec("false"), 0, t0);
        apply_event(&mut s, &command_finished(127), 0, t0);
        assert_eq!(s.get(0).unwrap().exit_code, Some(127));
    }

    #[test]
    fn precmd_without_command_finished_seals_with_no_exit() {
        // Ctrl-C path: user kills a running command, shell skips
        // CommandFinished and goes straight to the next Precmd.
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        apply_event(&mut s, &precmd(None), 0, t0);
        apply_event(&mut s, &preexec("sleep 100"), 0, t0);
        apply_event(&mut s, &precmd(None), 0, t0);
        assert_eq!(s.len(), 2);
        let first = s.get(0).unwrap();
        assert!(!first.is_running);
        assert!(first.exit_code.is_none());
        assert!(first.duration_ms.is_none());
    }

    #[test]
    fn preexec_without_precmd_synthesises_open() {
        let mut s = BlockStore::new();
        apply_event(&mut s, &preexec("date"), 9, Instant::now());
        assert_eq!(s.len(), 1);
        assert_eq!(s.get(0).unwrap().command, "date");
        assert!(s.get(0).unwrap().is_running);
    }

    #[test]
    fn command_finished_without_preexec_seals_without_duration() {
        // Partial-integration path: a CommandFinished arrives but no
        // matching Preexec was seen. Seal anyway with no duration.
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        apply_event(&mut s, &precmd(None), 0, t0);
        apply_event(
            &mut s,
            &command_finished(0),
            0,
            t0 + Duration::from_millis(50),
        );
        let b = s.get(0).unwrap();
        assert!(!b.is_running);
        assert_eq!(b.exit_code, Some(0));
        assert!(b.duration_ms.is_none());
    }

    #[test]
    fn reset_clears_all() {
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        apply_event(&mut s, &precmd(None), 0, t0);
        apply_event(&mut s, &preexec("ls"), 0, t0);
        apply_event(&mut s, &command_finished(0), 0, t0);
        assert!(!s.is_empty());
        s.reset();
        assert!(s.is_empty());
    }

    #[test]
    fn block_ids_are_monotonic_across_reset() {
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        apply_event(&mut s, &precmd(None), 0, t0);
        let first_id = s.get(0).unwrap().id;
        s.reset();
        apply_event(&mut s, &precmd(None), 0, t0);
        assert!(s.get(0).unwrap().id > first_id);
    }

    // ---------------------------------------------------------------------------
    // Direct Block construction, field access, and equality
    // ---------------------------------------------------------------------------

    #[test]
    fn construct_running_block() {
        let block = Block {
            id: 1,
            command: String::new(),
            start_line: 0,
            end_line: BLOCK_END_LINE_RUNNING,
            frozen_snapshot: None,
            exit_code: None,
            duration_ms: None,
            working_directory: Some("/home".into()),
            git_branch: Some("main".into()),
            cli_agent: None,
            grid: None,
            is_running: true,
            stylized_command: vec![],
            stylized_output: vec![],
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        };
        assert_eq!(block.id, 1);
        assert_eq!(block.end_line, BLOCK_END_LINE_RUNNING);
        assert!(block.is_running);
        assert!(block.frozen_snapshot.is_none());
        assert!(block.exit_code.is_none());
        assert!(block.duration_ms.is_none());
        assert_eq!(block.working_directory.as_deref(), Some("/home"));
        assert_eq!(block.git_branch.as_deref(), Some("main"));
        assert!(block.cli_agent.is_none());
        assert!(block.stylized_command.is_empty());
        assert!(block.stylized_output.is_empty());
        assert_eq!(block.stylized_command_lines, 0);
        assert_eq!(block.stylized_output_lines, 0);
    }

    #[test]
    fn construct_sealed_block_with_snapshot() {
        let snapshot = RawGridSnapshot {
            cols: 80,
            rows: 24,
            cursor_col: 0,
            cursor_row: 0,
            display_offset: 0,
            cells: vec![],
        };
        let block = Block {
            id: 2,
            command: "make".into(),
            start_line: 10,
            end_line: 20,
            frozen_snapshot: Some(snapshot),
            exit_code: Some(0),
            duration_ms: Some(5000),
            working_directory: None,
            git_branch: None,
            cli_agent: Some(CliAgent::Claude),
            grid: None,
            is_running: false,
            stylized_command: vec![],
            stylized_output: vec![],
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        };
        assert_eq!(block.id, 2);
        assert_eq!(block.command, "make");
        assert_eq!(block.start_line, 10);
        assert_eq!(block.end_line, 20);
        assert!(block.frozen_snapshot.is_some());
        assert_eq!(block.exit_code, Some(0));
        assert_eq!(block.duration_ms, Some(5000));
        assert!(block.working_directory.is_none());
        assert!(block.git_branch.is_none());
        assert_eq!(block.cli_agent, Some(CliAgent::Claude));
        assert!(!block.is_running);
    }

    #[test]
    fn construct_block_with_exit_code_and_duration() {
        // Non-zero exit code, edge-case duration values
        let block = Block {
            id: 3,
            command: "false".into(),
            start_line: 5,
            end_line: 5,
            frozen_snapshot: None,
            exit_code: Some(127),
            duration_ms: Some(0),
            working_directory: None,
            git_branch: None,
            cli_agent: None,
            grid: None,
            is_running: false,
            stylized_command: vec![],
            stylized_output: vec![],
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        };
        assert_eq!(block.exit_code, Some(127));
        assert_eq!(block.duration_ms, Some(0));
        // start_line == end_line means zero-height (collapsed) sealed block
        assert_eq!(block.end_line - block.start_line, 0);
    }

    #[test]
    fn construct_block_with_git_branch_and_working_directory() {
        let block = Block {
            id: 4,
            command: "git status".into(),
            start_line: 3,
            end_line: 7,
            frozen_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(100),
            working_directory: Some("/Users/alice/project".into()),
            git_branch: Some("feature/new-ui".into()),
            cli_agent: None,
            grid: None,
            is_running: false,
            stylized_command: vec![],
            stylized_output: vec![],
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        };
        assert_eq!(
            block.working_directory.as_deref(),
            Some("/Users/alice/project")
        );
        assert_eq!(block.git_branch.as_deref(), Some("feature/new-ui"));
    }

    #[test]
    fn block_fields_are_accessible() {
        // Access every public field by reading it back after construction.
        let block = Block {
            id: 100,
            command: "git diff".into(),
            start_line: 5,
            end_line: 12,
            frozen_snapshot: Some(RawGridSnapshot {
                cols: 80,
                rows: 24,
                cursor_col: 0,
                cursor_row: 0,
                display_offset: 0,
                cells: vec![],
            }),
            exit_code: Some(1),
            duration_ms: Some(200),
            working_directory: Some("/repo".into()),
            git_branch: Some("feature".into()),
            cli_agent: Some(CliAgent::Goose),
            grid: None,
            is_running: false,
            stylized_command: b"git diff".to_vec(),
            stylized_output: b"+new\n-removed".to_vec(),
            stylized_command_lines: 0,
            stylized_output_lines: 2,
        };
        assert_eq!(block.id, 100);
        assert_eq!(block.command, "git diff");
        assert_eq!(block.start_line, 5);
        assert_eq!(block.end_line, 12);
        assert!(block.frozen_snapshot.is_some());
        assert_eq!(block.frozen_snapshot.as_ref().unwrap().cols, 80);
        assert_eq!(block.frozen_snapshot.as_ref().unwrap().rows, 24);
        assert_eq!(block.exit_code, Some(1));
        assert_eq!(block.duration_ms, Some(200));
        assert_eq!(block.working_directory.as_deref(), Some("/repo"));
        assert_eq!(block.git_branch.as_deref(), Some("feature"));
        assert_eq!(block.cli_agent, Some(CliAgent::Goose));
        assert!(!block.is_running);
        assert_eq!(&block.stylized_command, b"git diff");
        assert_eq!(&block.stylized_output, b"+new\n-removed");
        assert_eq!(block.stylized_command_lines, 0);
        assert_eq!(block.stylized_output_lines, 2);
    }

    #[test]
    fn block_equality_by_fields() {
        // Two independently constructed blocks with identical field values
        // must compare equal field-by-field. (Block cannot derive PartialEq
        // because BlockGrid does not impl it.)
        let snap = RawGridSnapshot {
            cols: 80,
            rows: 24,
            cursor_col: 0,
            cursor_row: 0,
            display_offset: 0,
            cells: vec![],
        };
        let raw = |is_running| Block {
            id: 7,
            command: "test".into(),
            start_line: 1,
            end_line: if is_running {
                BLOCK_END_LINE_RUNNING
            } else {
                3
            },
            frozen_snapshot: if is_running { None } else { Some(snap.clone()) },
            exit_code: if is_running { None } else { Some(0) },
            duration_ms: if is_running { None } else { Some(500) },
            working_directory: Some("/tmp".into()),
            git_branch: Some("fix".into()),
            cli_agent: Some(CliAgent::Codex),
            grid: None,
            is_running,
            stylized_command: b"test".to_vec(),
            stylized_output: b"ok".to_vec(),
            stylized_command_lines: 0,
            stylized_output_lines: 1,
        };
        let sealed = raw(false);
        let sealed2 = raw(false);
        let running = raw(true);
        let running2 = raw(true);

        // Sealed vs sealed
        assert_eq!(sealed.id, sealed2.id);
        assert_eq!(sealed.command, sealed2.command);
        assert_eq!(sealed.start_line, sealed2.start_line);
        assert_eq!(sealed.end_line, sealed2.end_line);
        assert_eq!(sealed.exit_code, sealed2.exit_code);
        assert_eq!(sealed.duration_ms, sealed2.duration_ms);
        assert_eq!(sealed.working_directory, sealed2.working_directory);
        assert_eq!(sealed.git_branch, sealed2.git_branch);
        assert_eq!(sealed.cli_agent, sealed2.cli_agent);
        assert!(!sealed.is_running);
        assert!(!sealed2.is_running);
        assert_eq!(sealed.stylized_command, sealed2.stylized_command);
        assert_eq!(sealed.stylized_output, sealed2.stylized_output);

        // Running vs running
        assert_eq!(running.id, running2.id);
        assert_eq!(running.end_line, BLOCK_END_LINE_RUNNING);
        assert_eq!(running.end_line, running2.end_line);
        assert_eq!(
            running.frozen_snapshot.is_some(),
            running2.frozen_snapshot.is_some()
        );
        assert!(running.frozen_snapshot.is_none());
        assert_eq!(running.exit_code, running2.exit_code);
        assert_eq!(running.cli_agent, running2.cli_agent);
        assert!(running.is_running);
        assert!(running2.is_running);

        // Running and sealed differ in running-relevant fields
        assert_ne!(sealed.is_running, running.is_running);
        assert_ne!(sealed.end_line, running.end_line);
        assert_ne!(
            sealed.frozen_snapshot.is_some(),
            running.frozen_snapshot.is_some()
        );
        assert_ne!(sealed.exit_code, running.exit_code);
    }

    #[test]
    fn block_running_sentinel_is_i32_min() {
        assert_eq!(BLOCK_END_LINE_RUNNING, i32::MIN);
    }

    #[test]
    fn append_output_bytes_caps_at_max_lines() {
        let mut block = Block {
            id: 10,
            command: String::new(),
            start_line: 0,
            end_line: BLOCK_END_LINE_RUNNING,
            frozen_snapshot: None,
            exit_code: None,
            duration_ms: None,
            working_directory: None,
            git_branch: None,
            cli_agent: None,
            grid: None,
            is_running: true,
            stylized_command: vec![],
            stylized_output: vec![],
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        };
        // Feed more newlines than MAX_BLOCK_OUTPUT_LINES
        let overflow = (MAX_BLOCK_OUTPUT_LINES + 50) as usize;
        let mut many_bytes = vec![0u8; overflow];
        for i in 0..overflow {
            many_bytes[i] = b'\n';
        }
        block.append_output_bytes(&many_bytes);

        assert_eq!(block.stylized_output_lines, MAX_BLOCK_OUTPUT_LINES);
        // Every byte in the buffer should be a newline (the first
        // MAX_BLOCK_OUTPUT_LINES were all \n)
        assert_eq!(block.stylized_output.len(), MAX_BLOCK_OUTPUT_LINES as usize);
        assert!(block.stylized_output.iter().all(|&b| b == b'\n'));
    }

    #[test]
    fn append_command_bytes_works_independently() {
        let mut block = Block {
            id: 11,
            command: String::new(),
            start_line: 0,
            end_line: BLOCK_END_LINE_RUNNING,
            frozen_snapshot: None,
            exit_code: None,
            duration_ms: None,
            working_directory: None,
            git_branch: None,
            cli_agent: None,
            grid: None,
            is_running: true,
            stylized_command: vec![],
            stylized_output: vec![],
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        };

        block.append_command_bytes(b"echo hello\n");
        block.append_output_bytes(b"hello\nworld\n");

        assert_eq!(block.stylized_command, b"echo hello\n");
        assert_eq!(block.stylized_output, b"hello\nworld\n");
        assert_eq!(block.stylized_command_lines, 1);
        assert_eq!(block.stylized_output_lines, 2);
    }

    #[test]
    fn append_bytes_after_cap_are_dropped() {
        let mut block = Block {
            id: 12,
            command: String::new(),
            start_line: 0,
            end_line: BLOCK_END_LINE_RUNNING,
            frozen_snapshot: None,
            exit_code: None,
            duration_ms: None,
            working_directory: None,
            git_branch: None,
            cli_agent: None,
            grid: None,
            is_running: true,
            stylized_command: vec![],
            stylized_output: vec![],
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        };

        // Fill exactly to the cap
        let cap_bytes = vec![b'a'; MAX_BLOCK_OUTPUT_LINES as usize];
        block.append_output_bytes(&cap_bytes);
        assert_eq!(block.stylized_output_lines, 0); // no newlines, so counter unchanged
        assert_eq!(block.stylized_output.len(), MAX_BLOCK_OUTPUT_LINES as usize);

        // Now push past via newlines
        let newlines = vec![b'\n'; MAX_BLOCK_OUTPUT_LINES as usize];
        block.append_output_bytes(&newlines);
        assert_eq!(block.stylized_output_lines, MAX_BLOCK_OUTPUT_LINES);
        assert_eq!(
            block.stylized_output.len(),
            MAX_BLOCK_OUTPUT_LINES as usize * 2
        ); // 'a's + \n's

        // Everything beyond the cap is dropped; the cap fires on the
        // *newline* count, not on the vector length.
        let extra = b"\nsurvivor";
        block.append_output_bytes(extra);
        // output_lines should stay at MAX, and 'survivor' should never
        // appear because the first byte \n already hits the cap.
        assert_eq!(block.stylized_output_lines, MAX_BLOCK_OUTPUT_LINES);
        assert_eq!(
            block.stylized_output.len(),
            MAX_BLOCK_OUTPUT_LINES as usize * 2
        );
    }

    #[test]
    fn non_printable_bytes_in_stylized_buffers() {
        // Binary data in stylized buffers — null bytes, high bytes —
        // should be stored verbatim. The cap is per-byte regardless of value.
        let mut block = Block {
            id: 13,
            command: String::new(),
            start_line: 0,
            end_line: BLOCK_END_LINE_RUNNING,
            frozen_snapshot: None,
            exit_code: None,
            duration_ms: None,
            working_directory: None,
            git_branch: None,
            cli_agent: None,
            grid: None,
            is_running: true,
            stylized_command: vec![],
            stylized_output: vec![],
            stylized_command_lines: 0,
            stylized_output_lines: 0,
        };

        let mixed: &[u8] = &[0x00, 0x01, 0x1b, 0x7f, 0xFF, b'\n'];
        block.append_output_bytes(mixed);
        assert_eq!(block.stylized_output, mixed);
        assert_eq!(block.stylized_output_lines, 1);
    }

    #[test]
    fn each_cli_agent_variant_ffi_tag_is_consistent() {
        // The ffi_tag() values are stable (never renumbered). This test
        // catches accidental reordering of the CliAgent enum variants or
        // changes to the ffi_tag() discriminator.
        assert_eq!(CliAgent::Claude.ffi_tag(), 1);
        assert_eq!(CliAgent::Gemini.ffi_tag(), 2);
        assert_eq!(CliAgent::Codex.ffi_tag(), 3);
        assert_eq!(CliAgent::Amp.ffi_tag(), 4);
        assert_eq!(CliAgent::Droid.ffi_tag(), 5);
        assert_eq!(CliAgent::OpenCode.ffi_tag(), 6);
        assert_eq!(CliAgent::Copilot.ffi_tag(), 7);
        assert_eq!(CliAgent::Pi.ffi_tag(), 8);
        assert_eq!(CliAgent::Auggie.ffi_tag(), 9);
        assert_eq!(CliAgent::CursorCli.ffi_tag(), 10);
        assert_eq!(CliAgent::Goose.ffi_tag(), 11);
        assert_eq!(CliAgent::Hermes.ffi_tag(), 12);
        assert_eq!(CliAgent::Vibe.ffi_tag(), 13);
    }
}
