//! Port of `TerminalDisplayMode.swift`.
//!
//! Three top-level visual states the TerminalScreen can be in. Picked each
//! frame from `(altScreen flag, showCommandBlocks setting)`.
//!
//! - `BlockList` — Warp-style: only block cards + a flat always-open
//!   composer; the live terminal grid is hidden but still updated by the
//!   underlying TerminalCore so blocks keep growing.
//! - `Inline` — Classic terminal grid with subtle inline block decorations
//!   (dividers / badges) drawn over the cells; input is the raw PTY path
//!   (system keyboard → bytes).
//! - `AltScreen` — Full-screen TUI (vim / htop / claude); show the live
//!   grid only, no blocks, no Warp composer. Input is raw PTY.

#![allow(dead_code)]

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TerminalDisplayMode {
    BlockList,
    Inline,
    AltScreen,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_are_distinct() {
        assert_ne!(TerminalDisplayMode::BlockList, TerminalDisplayMode::Inline);
        assert_ne!(TerminalDisplayMode::Inline, TerminalDisplayMode::AltScreen);
        assert_ne!(
            TerminalDisplayMode::BlockList,
            TerminalDisplayMode::AltScreen
        );
    }
}
