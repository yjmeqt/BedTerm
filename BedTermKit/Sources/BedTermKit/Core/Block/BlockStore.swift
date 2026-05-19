import Foundation
import Observation

/// Observable mirror of the Rust-owned block list. After every
/// `TerminalCore.feed(_:)`, call `refresh(from:)` to repopulate from
/// FFI. The state machine itself lives in Rust now — Swift only
/// caches the most recent pull.
@MainActor
@Observable
public final class BlockStore {
    public private(set) var blocks: [Block] = []

    public init() {}

    public func refresh(from core: TerminalCore) {
        let rust = core.allBlocks()
        let unchanged =
            rust.count == blocks.count
            && zip(rust, blocks).allSatisfy { Self.fastEqual($0, $1) }
        guard !unchanged else { return }
        blocks = rust.map(Block.init(from:))
    }

    public func reset() {
        blocks.removeAll()
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
    }
}
