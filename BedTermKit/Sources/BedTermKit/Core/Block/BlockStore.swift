import Foundation
import Observation

/// Observable mirror of the Rust-owned block list. After every
/// `TerminalCore.feed(_:)`, call `refresh(from:)` to repopulate from
/// FFI. The DCS-driven state machine itself lives in Rust now — Swift
/// only caches the most recent pull.
@MainActor
@Observable
public final class BlockStore {
    public private(set) var blocks: [Block] = []
    /// Latest `pwd` shipped by any `Precmd` — including the pending
    /// block we hide from `blocks`. Observers (Block-list composer
    /// prompt strip) read this to keep the cwd chip live even before
    /// the first command runs.
    public private(set) var latestPwd: String?
    /// Same idea for the `git_branch` field.
    public private(set) var latestGitBranch: String?

    public init() {}

    public func refresh(from core: TerminalCore) {
        // Hide pending blocks (Precmd has opened a slot but Preexec
        // hasn't filled in a command yet). The shell always opens one
        // such block per prompt cycle — drawing it would put a permanent
        // "(no command)" card under the live prompt. Warp does the same:
        // the prompt itself in the live grid is the affordance, no card
        // needed.
        let all = core.allBlocks()
        let rust = all.filter { !($0.isRunning && $0.command.isEmpty) }
        // Refresh latest prompt context from the UNFILTERED list — the
        // pending block carries the freshest `pwd` / branch.
        let newPwd = all.reversed().lazy
            .compactMap { $0.workingDirectory }
            .first(where: { !$0.isEmpty })
        let newBranch = all.reversed().lazy
            .compactMap { $0.gitBranch }
            .first(where: { !$0.isEmpty })
        if newPwd != latestPwd { latestPwd = newPwd }
        if newBranch != latestGitBranch { latestGitBranch = newBranch }
        let unchanged =
            rust.count == blocks.count
            && zip(rust, blocks).allSatisfy { Self.fastEqual($0, $1) }
        guard !unchanged else { return }
        blocks = rust.map(Block.init(from:))
    }

    public func reset() {
        blocks.removeAll()
        latestPwd = nil
        latestGitBranch = nil
    }

    /// Equality fast-path: skip rebuild when Rust's view didn't actually
    /// change. Compares cheap scalar fields only; identical IDs imply the
    /// same command text by construction.
    private static func fastEqual(_ rust: RustBlock, _ mirror: Block) -> Bool {
        rust.id == mirror.id
            && rust.isRunning == mirror.isRunning
            && rust.endLine == (mirror.endLine ?? Int32.min)
            && rust.startLine == mirror.startLine
            && rust.hasFrozenSnapshot == mirror.hasFrozenSnapshot
            && rust.bodyRows == mirror.bodyRows
    }
}
