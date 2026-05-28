//! `BlockSource` — vtable for reading the live block list from the host
//! `TerminalSession`. In W4 the block list VC has no source wired and
//! returns zero blocks; W6 (Rust `TerminalSession`) lands a real impl.
//!
//! Modelled as a `Box<dyn BlockSource>` trait object rather than a plain
//! callback or C-style vtable because the layout pipeline needs random
//! access (`block_at(i)`) plus snapshot extraction at copy time — three
//! distinct calls per block. A trait object keeps the seams obvious and
//! lets W6 swap in a concrete struct without changing this crate.

#![allow(dead_code)]

use crate::block_header::CliAgent;

/// Per-block snapshot that the layout / header / selection paths need.
/// Mirrors the union of fields from Swift `Block` that the
/// `BlockListContainerViewController` family reads. The Rust renderer
/// itself never sees this struct — only the layout descriptor it derives.
#[derive(Clone, Debug)]
pub struct BlockSnapshot {
    /// Stable id used to key header / sticky / selection state.
    pub id: u64,
    /// Display command (the text the header band renders).
    pub command: String,
    /// Optional subtitle (duration / exit / pwd / branch).
    pub subtitle: Option<String>,
    /// Number of grid rows the body occupies.
    pub body_rows: u32,
    /// Whether the block is still running (drives display-link wakeup).
    pub is_running: bool,
    /// Whether a frozen grid snapshot is available for copy.
    pub has_frozen_snapshot: bool,
    /// CLI agent badge (drives header tint + icon slot).
    pub agent: Option<CliAgent>,
    /// PTY-line range of the block body — used by selection extraction
    /// to pull a `GridSnapshot` slice from `TerminalCore`.
    pub start_line: i32,
    pub end_line: Option<i32>,
}

/// Random-access read of the live block list.
///
/// All calls happen on the main thread (the block list VC drives them).
/// Implementations may borrow into the session's `BlockStore`; they
/// must not mutate during a layout pass.
pub trait BlockSource {
    fn block_count(&self) -> usize;
    fn block_at(&self, index: usize) -> Option<BlockSnapshot>;
}

/// Convenience: empty source used when no session is wired yet (W4).
pub struct EmptyBlockSource;

impl BlockSource for EmptyBlockSource {
    fn block_count(&self) -> usize {
        0
    }
    fn block_at(&self, _index: usize) -> Option<BlockSnapshot> {
        None
    }
}
