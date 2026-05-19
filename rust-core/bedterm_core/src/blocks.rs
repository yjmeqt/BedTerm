//! Warp-style command-block state machine. Consumes the OSC 133 event stream
//! emitted by `osc133::Osc133Sniffer` and produces a list of `Block`s: row
//! ranges into the shared terminal grid, sealed with a frozen `GridSnapshot`
//! at command-end so they survive scrollback eviction.

use crate::osc133::Osc133Event;
use crate::snapshot::GridSnapshot;

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
    /// `None` if the shell didn't ship one.
    pub exit_code: Option<i32>,
    /// `None` if the shell didn't ship `dur=`.
    pub duration_ms: Option<u64>,
    /// `None` if the shell didn't ship `cwd=`.
    pub working_directory: Option<String>,
    pub is_running: bool,
}

pub struct BlockStore {
    blocks: Vec<Block>,
    next_id: BlockId,
    open_index: Option<usize>,
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
        // `next_id` deliberately NOT reset — see doc on BlockId.
    }

    /// Apply one OSC 133 event. `current_line` is the cursor's current
    /// grid line (`Terminal::current_line()`); `snapshot_range` is a
    /// callback that produces the frozen body when sealing — the caller
    /// owns the live `Terminal` and is the only one who can read its
    /// grid. Returning `None` from the callback is fine (block seals
    /// with `frozen_snapshot = None`).
    pub fn apply<F>(&mut self, event: &Osc133Event, current_line: i32, mut snapshot_range: F)
    where
        F: FnMut(i32, i32) -> Option<GridSnapshot>,
    {
        match event {
            Osc133Event::PromptStart { attrs } => {
                // Seal an unclosed prior block (Ctrl-C path).
                self.seal_open(current_line, None, None, None, &mut snapshot_range);
                let cwd = decode_attr_b64(attrs, b"cwd");
                self.open_new(current_line, cwd);
            }
            Osc133Event::CommandStart { .. } => {
                // Informational; no state change. Required event for the
                // FinalTerm contract but our state machine doesn't react.
            }
            Osc133Event::OutputStart { attrs } => {
                if self.open_index.is_none() {
                    // Reconnect / partial integration: synthesise an open.
                    self.open_new(current_line, None);
                }
                if let Some(cmd) = decode_attr_b64(attrs, b"cmd") {
                    if let Some(idx) = self.open_index {
                        self.blocks[idx].command = cmd;
                    }
                }
            }
            Osc133Event::CommandEnd { exit_code, attrs } => {
                let dur_ms = decode_attr_u64(attrs, b"dur");
                let cwd = decode_attr_b64(attrs, b"cwd");
                self.seal_open(current_line, *exit_code, dur_ms, cwd, &mut snapshot_range);
            }
        }
    }

    fn open_new(&mut self, current_line: i32, cwd: Option<String>) {
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
            working_directory: cwd,
            is_running: true,
        });
        self.open_index = Some(self.blocks.len() - 1);
    }

    fn seal_open<F>(
        &mut self,
        current_line: i32,
        exit_code: Option<i32>,
        dur_ms: Option<u64>,
        cwd: Option<String>,
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
        block.duration_ms = dur_ms;
        if cwd.is_some() && block.working_directory.is_none() {
            block.working_directory = cwd;
        }
        self.open_index = None;
    }
}

/// Parse the sniffer's raw `attrs` payload (`key=value;key=value` bytes,
/// no leading/trailing semicolon) looking for `key`. Returns the value
/// slice (possibly empty) if found.
fn find_attr<'a>(attrs: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    for pair in attrs.split(|&b| b == b';') {
        if let Some(eq) = pair.iter().position(|&b| b == b'=') {
            if &pair[..eq] == key {
                return Some(&pair[eq + 1..]);
            }
        } else if pair == key {
            return Some(&[]);
        }
    }
    None
}

fn decode_attr_b64(attrs: &[u8], key: &[u8]) -> Option<String> {
    let raw = find_attr(attrs, key)?;
    if raw.is_empty() {
        return None;
    }
    let bytes = base64_decode(raw)?;
    String::from_utf8(bytes).ok()
}

fn decode_attr_u64(attrs: &[u8], key: &[u8]) -> Option<u64> {
    let raw = find_attr(attrs, key)?;
    std::str::from_utf8(raw).ok()?.parse().ok()
}

/// Tiny stdlib-only base64 decoder. We only feed it the well-formed
/// payloads our shell-integration scripts emit (and what third-party
/// integrations like iTerm2 emit); malformed input returns `None`.
fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let trimmed: Vec<u8> = input.iter().copied().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(trimmed.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in &trimmed {
        let v = val(b)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8 & 0xff);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn empty_on_init() {
        let s = BlockStore::new();
        assert!(s.is_empty());
    }

    #[test]
    fn prompt_start_opens_running_block() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 5, no_snap);
        assert_eq!(s.len(), 1);
        let b = s.get(0).unwrap();
        assert_eq!(b.start_line, 5);
        assert_eq!(b.end_line, BLOCK_END_LINE_RUNNING);
        assert!(b.is_running);
        assert!(b.frozen_snapshot.is_none());
    }

    #[test]
    fn output_start_fills_command() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 0, no_snap);
        // "bHM=" → "ls"
        s.apply(
            &Osc133Event::OutputStart {
                attrs: b"cmd=bHM=".to_vec(),
            },
            0,
            no_snap,
        );
        assert_eq!(s.get(0).unwrap().command, "ls");
    }

    #[test]
    fn command_end_seals_and_freezes() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 3, no_snap);
        s.apply(
            &Osc133Event::OutputStart {
                attrs: b"cmd=bHM=".to_vec(),
            },
            3,
            no_snap,
        );
        s.apply(
            &Osc133Event::CommandEnd {
                exit_code: Some(0),
                attrs: b"dur=1234".to_vec(),
            },
            7,
            snap_stub,
        );
        let b = s.get(0).unwrap();
        assert!(!b.is_running);
        assert_eq!(b.end_line, 8); // current_line + 1
        assert_eq!(b.exit_code, Some(0));
        assert_eq!(b.duration_ms, Some(1234));
        assert!(b.frozen_snapshot.is_some());
    }

    #[test]
    fn cwd_attr_from_prompt_start() {
        // base64 "/home/alice" = "L2hvbWUvYWxpY2U="
        let mut s = BlockStore::new();
        s.apply(
            &Osc133Event::PromptStart {
                attrs: b"cwd=L2hvbWUvYWxpY2U=".to_vec(),
            },
            0,
            no_snap,
        );
        assert_eq!(
            s.get(0).unwrap().working_directory.as_deref(),
            Some("/home/alice")
        );
    }

    #[test]
    fn nonzero_exit_carried() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 0, no_snap);
        s.apply(&Osc133Event::OutputStart { attrs: vec![] }, 0, no_snap);
        s.apply(
            &Osc133Event::CommandEnd {
                exit_code: Some(127),
                attrs: vec![],
            },
            0,
            no_snap,
        );
        assert_eq!(s.get(0).unwrap().exit_code, Some(127));
    }

    #[test]
    fn missing_exit_and_dur() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 0, no_snap);
        s.apply(&Osc133Event::OutputStart { attrs: vec![] }, 0, no_snap);
        s.apply(
            &Osc133Event::CommandEnd {
                exit_code: None,
                attrs: vec![],
            },
            0,
            no_snap,
        );
        let b = s.get(0).unwrap();
        assert!(b.exit_code.is_none());
        assert!(b.duration_ms.is_none());
    }

    #[test]
    fn new_prompt_seals_previous_if_running() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 1, no_snap);
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 4, no_snap);
        assert_eq!(s.len(), 2);
        assert!(!s.get(0).unwrap().is_running);
        assert!(s.get(0).unwrap().exit_code.is_none());
        assert_eq!(s.get(1).unwrap().start_line, 4);
    }

    #[test]
    fn output_start_opens_block_if_missing_prompt() {
        let mut s = BlockStore::new();
        s.apply(
            &Osc133Event::OutputStart {
                attrs: b"cmd=ZGF0ZQ==".to_vec(), // "date"
            },
            9,
            no_snap,
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s.get(0).unwrap().command, "date");
    }

    #[test]
    fn reset_clears_all() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 0, no_snap);
        s.apply(&Osc133Event::OutputStart { attrs: vec![] }, 0, no_snap);
        s.apply(
            &Osc133Event::CommandEnd {
                exit_code: Some(0),
                attrs: vec![],
            },
            0,
            no_snap,
        );
        assert!(!s.is_empty());
        s.reset();
        assert!(s.is_empty());
    }

    #[test]
    fn command_start_is_informational_only() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 0, no_snap);
        let before = s.len();
        s.apply(&Osc133Event::CommandStart { attrs: vec![] }, 0, no_snap);
        assert_eq!(s.len(), before);
        assert!(s.get(0).unwrap().is_running);
    }

    #[test]
    fn block_ids_are_monotonic_across_reset() {
        let mut s = BlockStore::new();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 0, no_snap);
        let first_id = s.get(0).unwrap().id;
        s.reset();
        s.apply(&Osc133Event::PromptStart { attrs: vec![] }, 0, no_snap);
        assert!(s.get(0).unwrap().id > first_id);
    }
}
