import Foundation
import Testing

@testable import BedTermKit

@MainActor
@Suite("BlockStore")
struct BlockStoreTests {
    /// Push a DCS byte stream through a fresh terminal + store pair and
    /// return the resulting (core, store).
    private func feed(_ bytes: Data) -> (TerminalCore, BlockStore) {
        let core = TerminalCore(cols: 80, rows: 24)
        let store = BlockStore()
        core.feed(bytes)
        store.refresh(from: core)
        return (core, store)
    }

    // MARK: - Wire-format helpers (must match `bedterm_core::dcs`).

    private func hex(_ input: String) -> String {
        input.utf8.map { String(format: "%02x", $0) }.joined()
    }

    /// Warp-compatible DCS frame: `ESC P $ d <hex(JSON)> ESC \`.
    private func dcs(_ json: String) -> Data {
        var bytes: [UInt8] = [0x1B, 0x50, 0x24, 0x64]  // ESC P $ d
        bytes.append(contentsOf: hex(json).utf8)
        bytes.append(contentsOf: [0x1B, 0x5C])  // ESC \
        return Data(bytes)
    }

    private func precmd(pwd: String = "") -> Data {
        dcs(#"{"hook":"Precmd","value":{"pwd":"\#(pwd)"}}"#)
    }

    private func preexec(_ command: String) -> Data {
        dcs(#"{"hook":"Preexec","value":{"command":"\#(command)"}}"#)
    }

    private func commandFinished(exit: Int32) -> Data {
        dcs(#"{"hook":"CommandFinished","value":{"exit_code":\#(exit)}}"#)
    }

    private func bytes(_ chunks: Data...) -> Data {
        var out = Data()
        for chunk in chunks { out.append(chunk) }
        return out
    }

    // MARK: - Lifecycle

    @Test("empty on init")
    func emptyOnInit() {
        let store = BlockStore()
        #expect(store.blocks.isEmpty)
    }

    @Test("first Precmd opens a running block")
    func firstPrecmdOpensRunningBlock() {
        let (_, store) = feed(precmd())
        #expect(store.blocks.count == 1)
        #expect(store.blocks[0].isRunning)
        #expect(store.blocks[0].endLine == nil)
        #expect(!store.blocks[0].hasFrozenSnapshot)
    }

    @Test("pwd parsed from Precmd")
    func pwdParsedFromPrecmd() {
        let (_, store) = feed(precmd(pwd: "/home/alice"))
        #expect(store.blocks.first?.workingDirectory == "/home/alice")
    }

    @Test("Preexec fills command")
    func preexecFillsCommand() {
        let (_, store) = feed(bytes(precmd(), preexec("ls")))
        #expect(store.blocks.first?.command == "ls")
    }

    @Test("CommandFinished seals and freezes")
    func commandFinishedSealsAndFreezes() throws {
        let (_, store) = feed(
            bytes(
                precmd(pwd: "/tmp"),
                preexec("ls"),
                commandFinished(exit: 0)
            ))
        let first = try #require(store.blocks.first)
        #expect(!first.isRunning)
        #expect(first.endLine != nil)
        #expect(first.exitCode == 0)
        // Duration is wall-clock — just assert it's measured.
        #expect(first.duration != nil)
        #expect(first.hasFrozenSnapshot)
    }

    @Test("non-zero exit code is carried")
    func nonzeroExitCarried() {
        let (_, store) = feed(
            bytes(precmd(), preexec("false"), commandFinished(exit: 127)))
        #expect(store.blocks.first?.exitCode == 127)
    }

    @Test("Ctrl-C path seals without exit")
    func ctrlCPathSealsWithoutExit() {
        // Precmd-following-Precmd (no CommandFinished in between) seals the
        // first block with no exit code — the Ctrl-C / partial-integration
        // path.
        let (_, store) = feed(bytes(precmd(), preexec("sleep 100"), precmd()))
        #expect(store.blocks.count == 2)
        #expect(!store.blocks[0].isRunning)
        #expect(store.blocks[0].exitCode == nil)
        #expect(store.blocks[0].duration == nil)
    }

    @Test("Preexec without Precmd synthesises an open block")
    func preexecWithoutPrecmdSynthesisesOpen() {
        let (_, store) = feed(preexec("date"))
        #expect(store.blocks.count == 1)
        #expect(store.blocks.first?.command == "date")
    }

    @Test("reset clears all blocks")
    func resetClearsAll() {
        let (_, store) = feed(
            bytes(precmd(), preexec("ls"), commandFinished(exit: 0)))
        #expect(!store.blocks.isEmpty)
        store.reset()
        #expect(store.blocks.isEmpty)
    }

    @Test("frozen snapshot fetchable after seal")
    func frozenSnapshotFetchableAfterSeal() throws {
        let (core, store) = feed(
            bytes(precmd(), preexec("ls"), commandFinished(exit: 0)))
        let block = try #require(store.blocks.first)
        #expect(block.hasFrozenSnapshot)
        let snap = core.frozenSnapshot(forBlockAt: 0)
        #expect(snap != nil)
    }

    @Test("other DCS traffic is ignored")
    func otherDcsTrafficIsIgnored() {
        // A Sixel-shaped DCS (`q` final byte) must NOT produce a block.
        let core = TerminalCore(cols: 80, rows: 24)
        let store = BlockStore()
        core.feed(Data([0x1B, 0x50, 0x71, 0x31, 0x3B, 0x1B, 0x5C]))
        store.refresh(from: core)
        #expect(store.blocks.isEmpty)
    }
}
