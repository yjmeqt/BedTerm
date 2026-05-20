import XCTest

@testable import BedTermKit

@MainActor
final class BlockStoreTests: XCTestCase {
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

    func testEmptyOnInit() {
        let store = BlockStore()
        XCTAssertTrue(store.blocks.isEmpty)
    }

    func testFirstPrecmdOpensRunningBlock() {
        let (_, store) = feed(precmd())
        XCTAssertEqual(store.blocks.count, 1)
        XCTAssertTrue(store.blocks[0].isRunning)
        XCTAssertNil(store.blocks[0].endLine)
        XCTAssertFalse(store.blocks[0].hasFrozenSnapshot)
    }

    func testPwdParsedFromPrecmd() {
        let (_, store) = feed(precmd(pwd: "/home/alice"))
        XCTAssertEqual(store.blocks.first?.workingDirectory, "/home/alice")
    }

    func testPreexecFillsCommand() {
        let (_, store) = feed(bytes(precmd(), preexec("ls")))
        XCTAssertEqual(store.blocks.first?.command, "ls")
    }

    func testCommandFinishedSealsAndFreezes() throws {
        let (_, store) = feed(
            bytes(
                precmd(pwd: "/tmp"),
                preexec("ls"),
                commandFinished(exit: 0)
            ))
        let first = try XCTUnwrap(store.blocks.first)
        XCTAssertFalse(first.isRunning)
        XCTAssertNotNil(first.endLine)
        XCTAssertEqual(first.exitCode, 0)
        // Duration is wall-clock — just assert it's measured.
        XCTAssertNotNil(first.duration)
        XCTAssertTrue(first.hasFrozenSnapshot)
    }

    func testNonzeroExitCarried() {
        let (_, store) = feed(
            bytes(precmd(), preexec("false"), commandFinished(exit: 127)))
        XCTAssertEqual(store.blocks.first?.exitCode, 127)
    }

    func testCtrlCPathSealsWithoutExit() {
        // Precmd-following-Precmd (no CommandFinished in between) seals the
        // first block with no exit code — the Ctrl-C / partial-integration
        // path.
        let (_, store) = feed(bytes(precmd(), preexec("sleep 100"), precmd()))
        XCTAssertEqual(store.blocks.count, 2)
        XCTAssertFalse(store.blocks[0].isRunning)
        XCTAssertNil(store.blocks[0].exitCode)
        XCTAssertNil(store.blocks[0].duration)
    }

    func testPreexecWithoutPrecmdSynthesisesOpen() {
        let (_, store) = feed(preexec("date"))
        XCTAssertEqual(store.blocks.count, 1)
        XCTAssertEqual(store.blocks.first?.command, "date")
    }

    func testResetClearsAll() {
        let (_, store) = feed(
            bytes(precmd(), preexec("ls"), commandFinished(exit: 0)))
        XCTAssertFalse(store.blocks.isEmpty)
        store.reset()
        XCTAssertTrue(store.blocks.isEmpty)
    }

    func testFrozenSnapshotFetchableAfterSeal() throws {
        let (core, store) = feed(
            bytes(precmd(), preexec("ls"), commandFinished(exit: 0)))
        let block = try XCTUnwrap(store.blocks.first)
        XCTAssertTrue(block.hasFrozenSnapshot)
        let snap = core.frozenSnapshot(forBlockAt: 0)
        XCTAssertNotNil(snap)
    }

    func testOtherDcsTrafficIsIgnored() {
        // A Sixel-shaped DCS (`q` final byte) must NOT produce a block.
        let core = TerminalCore(cols: 80, rows: 24)
        let store = BlockStore()
        core.feed(Data([0x1B, 0x50, 0x71, 0x31, 0x3B, 0x1B, 0x5C]))
        store.refresh(from: core)
        XCTAssertTrue(store.blocks.isEmpty)
    }
}
