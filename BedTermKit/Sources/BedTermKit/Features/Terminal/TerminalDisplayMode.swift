import Foundation

/// Three top-level visual states the TerminalScreen can be in. Picked
/// each frame from `(altScreen flag, showCommandBlocks setting)`.
///
/// - `.blockList` — Warp-style: only block cards + a flat always-open
///   composer; the live terminal grid is hidden but still updated by
///   the underlying TerminalCore so blocks keep growing.
/// - `.inline` — Classic terminal grid with subtle inline block
///   decorations (dividers / badges) drawn over the cells; input is
///   the raw PTY path (system keyboard → bytes).
/// - `.altScreen` — Full-screen TUI (vim / htop / claude); show the
///   live grid only, no blocks, no Warp composer. Input is raw PTY.
public enum TerminalDisplayMode: Equatable, Sendable {
    case blockList
    case inline
    case altScreen
}
