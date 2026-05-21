import Foundation

/// One Warp-style command block: the command, its row range in the
/// underlying terminal grid, and the metadata captured around it from
/// the bundled DCS shell-integration protocol.
///
/// Storage model: a block is **a row range within the shared terminal
/// grid**, not a separate byte-capture buffer. While the block is running
/// it re-snapshots `[startLine, currentCursorLine + 1)` from the grid on
/// every redraw — same Metal pipeline the live terminal view uses. When
/// the next prompt boundary arrives (the shell's `precmd` hook), the row
/// range is frozen into an immutable `GridSnapshot` so it survives
/// subsequent scrolling / scrollback eviction.
///
/// The canonical block list lives in the Rust core (`BlockStore`).
/// Swift's `BlockStore` is an observable mirror — see
/// `BlockStore.refresh(from:)`. Frozen snapshots are fetched on demand
/// from Rust via `TerminalCore.frozenSnapshot(forBlockAt:)`.
public struct Block: Identifiable, Sendable, Equatable {
    public let id: UInt64
    /// The command line as the remote shell saw it. Filled by the
    /// `bedterm_preexec` DCS event (`{"cmd":"<hex>"}`). Empty until then.
    public var command: String
    /// Grid line where the block started — the cursor line at the
    /// `bedterm_precmd` boundary that opened this block. Used by the
    /// running-block renderer to compute the row range to draw.
    public var startLine: Int32
    /// Grid line one past the last row that belongs to this block. `nil`
    /// while the block is still running.
    public var endLine: Int32?
    /// Exit code from the next `bedterm_precmd`'s `exit` field. `nil` when
    /// the shell didn't ship it (first-of-session or partial integration).
    public var exitCode: Int32?
    /// Wall-clock duration from the next `bedterm_precmd`'s `dur_ms`. `nil`
    /// when not shipped.
    public var duration: TimeInterval?
    /// Working directory from `bedterm_precmd`'s `cwd` field. `nil` when
    /// not shipped.
    public var workingDirectory: String?
    /// Git branch (short name / short SHA) from `bedterm_precmd`'s
    /// `git_branch` field. `nil` when cwd is outside a repo, `git` is
    /// missing remotely, or the shell-integration script hasn't been
    /// installed yet (older payloads).
    public var gitBranch: String?
    /// Known CLI agent (Claude / Codex / Gemini / …) identified from
    /// the command line at `Preexec`. `nil` for unrecognised
    /// commands. Used for the brand icon next to the header; does
    /// not influence layout.
    public var cliAgent: CLIAgent?
    /// True from creation until the closing precmd event arrives.
    public var isRunning: Bool
    /// True once Rust has captured an immutable `GridSnapshot` for this
    /// block. The snapshot itself lives in Rust; fetch it on demand with
    /// `TerminalCore.frozenSnapshot(forBlockAt:)`.
    public var hasFrozenSnapshot: Bool
    /// Live body extent in grid rows reported by Rust. Use this — not
    /// `endLine - startLine` — when sizing the block body in points;
    /// for running blocks Rust updates it every feed so a streaming
    /// TUI grows the block in real time without pre-allocating the
    /// full PTY screen height.
    public var bodyRows: UInt32

    public init(
        id: UInt64,
        command: String = "",
        startLine: Int32 = 0,
        endLine: Int32? = nil,
        exitCode: Int32? = nil,
        duration: TimeInterval? = nil,
        workingDirectory: String? = nil,
        gitBranch: String? = nil,
        cliAgent: CLIAgent? = nil,
        isRunning: Bool = true,
        hasFrozenSnapshot: Bool = false,
        bodyRows: UInt32 = 1
    ) {
        self.id = id
        self.command = command
        self.startLine = startLine
        self.endLine = endLine
        self.exitCode = exitCode
        self.duration = duration
        self.workingDirectory = workingDirectory
        self.gitBranch = gitBranch
        self.cliAgent = cliAgent
        self.isRunning = isRunning
        self.hasFrozenSnapshot = hasFrozenSnapshot
        self.bodyRows = bodyRows
    }
}

extension Block {
    init(from rust: RustBlock) {
        self.init(
            id: rust.id,
            command: rust.command,
            startLine: rust.startLine,
            endLine: rust.isRunning ? nil : rust.endLine,
            exitCode: rust.exitCode,
            duration: rust.duration,
            workingDirectory: rust.workingDirectory,
            gitBranch: rust.gitBranch,
            cliAgent: rust.cliAgent,
            isRunning: rust.isRunning,
            hasFrozenSnapshot: rust.hasFrozenSnapshot,
            bodyRows: rust.bodyRows
        )
    }
}
