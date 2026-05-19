import XCTest

@testable import BedTermKit

@MainActor
final class BlockStoreTests: XCTestCase {
    /// Helper: send raw bytes through a fresh terminal + store pair and
    /// return the resulting (core, store) so the test can assert on either.
    private func feed(_ bytes: String) -> (TerminalCore, BlockStore) {
        let core = TerminalCore(cols: 80, rows: 24)
        let store = BlockStore()
        core.feed(Data(bytes.utf8))
        store.refresh(from: core)
        return (core, store)
    }

    // OSC 133 helpers — ST is ESC \
    private static let osc = "\u{1B}]"
    private static let st = "\u{1B}\\"

    private func promptStart(cwd: String? = nil) -> String {
        if let cwd {
            let b64 = Data(cwd.utf8).base64EncodedString()
            return "\(Self.osc)133;A;cwd=\(b64)\(Self.st)"
        }
        return "\(Self.osc)133;A\(Self.st)"
    }

    private func outputStart(cmd: String? = nil) -> String {
        if let cmd {
            let b64 = Data(cmd.utf8).base64EncodedString()
            return "\(Self.osc)133;C;cmd=\(b64)\(Self.st)"
        }
        return "\(Self.osc)133;C\(Self.st)"
    }

    private func commandEnd(exit: Int? = nil, durMs: UInt64? = nil) -> String {
        var seq = "\(Self.osc)133;D"
        if let exit { seq += ";\(exit)" }
        if let durMs { seq += ";dur=\(durMs)" }
        seq += Self.st
        return seq
    }

    // MARK: - Lifecycle

    func testEmptyOnInit() {
        let store = BlockStore()
        XCTAssertTrue(store.blocks.isEmpty)
    }

    func testPromptStartOpensRunningBlock() {
        let (_, store) = feed(promptStart())
        XCTAssertEqual(store.blocks.count, 1)
        XCTAssertTrue(store.blocks[0].isRunning)
        XCTAssertNil(store.blocks[0].endLine)
        XCTAssertFalse(store.blocks[0].hasFrozenSnapshot)
    }

    func testOutputStartFillsCommand() {
        let (_, store) = feed(promptStart() + outputStart(cmd: "ls"))
        XCTAssertEqual(store.blocks.first?.command, "ls")
    }

    func testCommandEndSealsAndFreezes() throws {
        let (_, store) = feed(
            promptStart()
                + outputStart(cmd: "ls")
                + commandEnd(exit: 0, durMs: 1234)
        )
        let block = try XCTUnwrap(store.blocks.first)
        XCTAssertFalse(block.isRunning)
        XCTAssertNotNil(block.endLine)
        XCTAssertEqual(block.exitCode, 0)
        XCTAssertEqual(try XCTUnwrap(block.duration), 1.234, accuracy: 0.001)
        XCTAssertTrue(block.hasFrozenSnapshot)
    }

    func testCwdAttrParsedFromPromptStart() {
        let (_, store) = feed(promptStart(cwd: "/home/alice"))
        XCTAssertEqual(store.blocks.first?.workingDirectory, "/home/alice")
    }

    func testNonzeroExitCarried() {
        let (_, store) = feed(promptStart() + outputStart() + commandEnd(exit: 127))
        XCTAssertEqual(store.blocks.first?.exitCode, 127)
    }

    func testMissingExitAndDur() {
        let (_, store) = feed(promptStart() + outputStart() + commandEnd())
        XCTAssertNil(store.blocks.first?.exitCode)
        XCTAssertNil(store.blocks.first?.duration)
    }

    func testNewPromptSealsPreviousIfRunning() {
        let (_, store) = feed(promptStart() + promptStart())
        XCTAssertEqual(store.blocks.count, 2)
        XCTAssertFalse(store.blocks[0].isRunning)
        XCTAssertNil(store.blocks[0].exitCode)
    }

    func testOutputStartOpensBlockIfMissingPromptStart() {
        let (_, store) = feed(outputStart(cmd: "date"))
        XCTAssertEqual(store.blocks.count, 1)
        XCTAssertEqual(store.blocks.first?.command, "date")
    }

    func testResetClearsAll() {
        let (_, store) = feed(promptStart() + outputStart() + commandEnd(exit: 0))
        XCTAssertFalse(store.blocks.isEmpty)
        store.reset()
        XCTAssertTrue(store.blocks.isEmpty)
    }

    func testCommandStartIsInformationalOnly() {
        // 133;B between A and C; the block stays running until D arrives.
        let stream = promptStart() + "\u{1B}]133;B\u{1B}\\" + outputStart()
        let (_, store) = feed(stream)
        XCTAssertEqual(store.blocks.count, 1)
        XCTAssertTrue(store.blocks[0].isRunning)
    }

    func testFrozenSnapshotFetchableAfterSeal() throws {
        let (core, store) = feed(
            promptStart() + outputStart(cmd: "ls") + commandEnd(exit: 0)
        )
        let block = try XCTUnwrap(store.blocks.first)
        XCTAssertTrue(block.hasFrozenSnapshot)
        let snap = core.frozenSnapshot(forBlockAt: 0)
        XCTAssertNotNil(snap)
    }
}
