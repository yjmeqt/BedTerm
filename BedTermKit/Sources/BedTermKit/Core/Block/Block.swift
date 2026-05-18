import Foundation

/// One Warp-style command block: the command, its row range in the
/// underlying terminal grid, and the metadata captured around it via OSC
/// 133 (FinalTerm) markers.
///
/// Storage model: a block is **a row range within the shared terminal
/// grid**, not a separate byte-capture buffer. While the block is running
/// it re-snapshots `[startLine, currentCursorLine + 1)` from the grid on
/// every redraw — same Metal pipeline the live terminal view uses. On
/// `OSC 133 ; D` (command end) the row range is frozen into an immutable
/// `GridSnapshot` so it survives subsequent scrolling / scrollback
/// eviction.
///
/// This mirrors Warp's approach: one terminal grid, blocks are slices
/// into it. Full ANSI colours / wide chars / line wrapping fall out for
/// free because alacritty already classifies every cell.
public struct Block: Identifiable, Sendable, Equatable {
    public let id: UUID
    /// The command line as the remote shell saw it. Empty until our
    /// bundled integration script ships it via `cmd=<base64>` on
    /// `OSC 133 ; C`; third-party integrations typically don't include
    /// it (and the UI shows a placeholder).
    public var command: String
    /// Grid line where the block started (the line of the prompt at the
    /// instant `OSC 133 ; A` arrived). Used by the running-block renderer
    /// to compute the row range to draw — start..currentCursorLine+1.
    public var startLine: Int32
    /// Grid line one past the last row that belongs to this block. `nil`
    /// while the block is still running.
    public var endLine: Int32?
    /// Frozen snapshot of `[startLine, endLine)` taken at command-end.
    /// `nil` while running. The frozen copy survives scrollback eviction
    /// and is what the Block list renders for sealed blocks.
    public var frozenSnapshot: GridSnapshot?
    /// Exit code from `OSC 133 ; D`'s positional-2 slot. `nil` when the
    /// integration didn't include it.
    public var exitCode: Int32?
    /// Wall-clock duration from `dur=<millis>` attr. `nil` when not shipped.
    public var duration: TimeInterval?
    /// Working directory from `cwd=<base64>` attr. `nil` when not shipped.
    public var workingDirectory: String?
    /// True from creation until the closing event arrives.
    public var isRunning: Bool

    public init(
        id: UUID = UUID(),
        command: String = "",
        startLine: Int32 = 0,
        endLine: Int32? = nil,
        frozenSnapshot: GridSnapshot? = nil,
        exitCode: Int32? = nil,
        duration: TimeInterval? = nil,
        workingDirectory: String? = nil,
        isRunning: Bool = true
    ) {
        self.id = id
        self.command = command
        self.startLine = startLine
        self.endLine = endLine
        self.frozenSnapshot = frozenSnapshot
        self.exitCode = exitCode
        self.duration = duration
        self.workingDirectory = workingDirectory
        self.isRunning = isRunning
    }
}
