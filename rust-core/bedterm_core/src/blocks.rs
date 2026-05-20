//! Warp-style command-block state machine. Consumes the DCS event stream
//! emitted by `dcs::DcsSniffer` and produces a list of `Block`s: row
//! ranges into the shared terminal grid, sealed with a frozen `GridSnapshot`
//! at command-end so they survive scrollback eviction.
//!
//! Wall-clock duration is computed on the Rust side as the delta between
//! `Preexec` and `CommandFinished` boundaries — the shell doesn't ship a
//! `duration_ms` field (and neither does Warp's wire format).

use crate::cli_agent::CliAgent;
use crate::dcs::DcsEvent;
use crate::snapshot::GridSnapshot;
use std::time::Instant;

/// Stable monotonic block id, assigned at open. Never reused inside a single
/// `BlockStore`; survives `reset()` by continuing to count up so Swift `@Observable`
/// diffs stay clean across reconnects.
pub type BlockId = u64;

/// Sentinel for a still-running block's `end_line`. Picked outside the legal
/// `i32` grid-line range alacritty produces. Public so the FFI layer can
/// surface it to Swift.
pub const BLOCK_END_LINE_RUNNING: i32 = i32::MIN;

#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    pub command: String,
    pub start_line: i32,
    /// `BLOCK_END_LINE_RUNNING` while the block is still running.
    pub end_line: i32,
    /// `None` while running.
    pub frozen_snapshot: Option<GridSnapshot>,
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
    pub is_running: bool,
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
    /// - `now` is the wall-clock instant the event arrived; used to
    ///   compute `duration_ms` across `Preexec` → `CommandFinished`. The
    ///   parameter lets tests inject deterministic clocks; production
    ///   callers pass `Instant::now()`.
    /// - `snapshot_range` produces the frozen body when sealing — the caller
    ///   owns the live `Terminal` and is the only one who can read its grid.
    pub fn apply<F>(
        &mut self,
        event: &DcsEvent,
        current_line: i32,
        now: Instant,
        mut snapshot_range: F,
    ) where
        F: FnMut(i32, i32) -> Option<GridSnapshot>,
    {
        match event {
            DcsEvent::Precmd { pwd, git_branch } => {
                // A bare Precmd means "about to draw a prompt". If a block
                // is still open it never got a CommandFinished (Ctrl-C,
                // partial integration); seal it with no exit + no duration.
                self.seal_open(current_line, None, &mut snapshot_range);
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
                }
                self.command_start = Some(now);
            }
            DcsEvent::CommandFinished { exit_code } => {
                let duration_ms = self
                    .command_start
                    .take()
                    .map(|start| now.saturating_duration_since(start).as_millis() as u64);
                self.seal_with(
                    current_line,
                    Some(*exit_code),
                    duration_ms,
                    &mut snapshot_range,
                );
            }
        }
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
            is_running: true,
        });
        self.open_index = Some(self.blocks.len() - 1);
    }

    /// Seal the currently-open block (if any) without exit/duration —
    /// used when a Precmd arrives over an already-running block.
    fn seal_open<F>(&mut self, current_line: i32, exit_code: Option<i32>, snapshot_range: &mut F)
    where
        F: FnMut(i32, i32) -> Option<GridSnapshot>,
    {
        self.seal_with(current_line, exit_code, None, snapshot_range);
    }

    fn seal_with<F>(
        &mut self,
        current_line: i32,
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
        snapshot_range: &mut F,
    ) where
        F: FnMut(i32, i32) -> Option<GridSnapshot>,
    {
        let Some(idx) = self.open_index else { return };
        if !self.blocks[idx].is_running {
            return;
        }
        let end_line = current_line.saturating_add(1);
        let snap = snapshot_range(self.blocks[idx].start_line, end_line);
        let block = &mut self.blocks[idx];
        block.is_running = false;
        block.end_line = end_line;
        block.frozen_snapshot = snap;
        block.exit_code = exit_code;
        block.duration_ms = duration_ms;
        self.open_index = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn snap_stub(_start: i32, _end: i32) -> Option<GridSnapshot> {
        Some(GridSnapshot {
            cols: 1,
            rows: 1,
            cursor_col: 0,
            cursor_row: 1,
            display_offset: 0,
            cells: vec![crate::snapshot::CellSnapshot {
                ch: 'A' as u32,
                fg_rgba: 0,
                bg_rgba: 0,
                flags: 0,
            }],
        })
    }

    fn no_snap(_start: i32, _end: i32) -> Option<GridSnapshot> {
        None
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
        s.apply(&precmd(Some("/home/alice")), 5, Instant::now(), no_snap);
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
        s.apply(&precmd(None), 0, t0, no_snap);
        s.apply(&preexec("ls"), 0, t0, no_snap);
        assert_eq!(s.get(0).unwrap().command, "ls");
    }

    #[test]
    fn command_finished_seals_and_records_duration() {
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        s.apply(&precmd(Some("/tmp")), 3, t0, no_snap);
        s.apply(&preexec("ls"), 3, t0, no_snap);
        s.apply(
            &command_finished(0),
            7,
            t0 + Duration::from_millis(1234),
            snap_stub,
        );
        let first = s.get(0).unwrap();
        assert!(!first.is_running);
        assert_eq!(first.end_line, 8);
        assert_eq!(first.exit_code, Some(0));
        assert_eq!(first.duration_ms, Some(1234));
        assert!(first.frozen_snapshot.is_some());
    }

    #[test]
    fn nonzero_exit_carried() {
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        s.apply(&precmd(None), 0, t0, no_snap);
        s.apply(&preexec("false"), 0, t0, no_snap);
        s.apply(&command_finished(127), 0, t0, no_snap);
        assert_eq!(s.get(0).unwrap().exit_code, Some(127));
    }

    #[test]
    fn precmd_without_command_finished_seals_with_no_exit() {
        // Ctrl-C path: user kills a running command, shell skips
        // CommandFinished and goes straight to the next Precmd.
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        s.apply(&precmd(None), 0, t0, no_snap);
        s.apply(&preexec("sleep 100"), 0, t0, no_snap);
        s.apply(&precmd(None), 0, t0, no_snap);
        assert_eq!(s.len(), 2);
        let first = s.get(0).unwrap();
        assert!(!first.is_running);
        assert!(first.exit_code.is_none());
        assert!(first.duration_ms.is_none());
    }

    #[test]
    fn preexec_without_precmd_synthesises_open() {
        let mut s = BlockStore::new();
        s.apply(&preexec("date"), 9, Instant::now(), no_snap);
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
        s.apply(&precmd(None), 0, t0, no_snap);
        s.apply(
            &command_finished(0),
            0,
            t0 + Duration::from_millis(50),
            no_snap,
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
        s.apply(&precmd(None), 0, t0, no_snap);
        s.apply(&preexec("ls"), 0, t0, no_snap);
        s.apply(&command_finished(0), 0, t0, no_snap);
        assert!(!s.is_empty());
        s.reset();
        assert!(s.is_empty());
    }

    #[test]
    fn block_ids_are_monotonic_across_reset() {
        let mut s = BlockStore::new();
        let t0 = Instant::now();
        s.apply(&precmd(None), 0, t0, no_snap);
        let first_id = s.get(0).unwrap().id;
        s.reset();
        s.apply(&precmd(None), 0, t0, no_snap);
        assert!(s.get(0).unwrap().id > first_id);
    }
}
