import XCTest

@testable import BedTermKit

@MainActor
final class BlockStoreTests: XCTestCase {
    /// Recordable doubles for the line + snapshot accessors, so we can
    /// simulate a moving cursor and verify the store records boundaries
    /// at the right grid lines.
    final class Probe {
        var currentLine: Int32 = 0
        var snapshotCalls: [(start: Int32, end: Int32)] = []
        var snapshotResult: GridSnapshot? = GridSnapshot(
            cols: 1, rows: 1, cursorCol: 0, cursorRow: 1,
            cells: [GridSnapshot.Cell(ch: 0x41, fgRGBA: 0, bgRGBA: 0, flags: 0)]
        )
    }

    private func makeStore(_ probe: Probe) -> BlockStore {
        let store = BlockStore()
        store.bind(
            currentLine: { probe.currentLine },
            snapshotRange: { start, end in
                probe.snapshotCalls.append((start, end))
                return probe.snapshotResult
            }
        )
        return store
    }

    // MARK: - Lifecycle

    func testEmptyOnInit() {
        let store = makeStore(Probe())
        XCTAssertTrue(store.blocks.isEmpty)
    }

    func testPromptStartOpensRunningBlock() {
        let probe = Probe()
        probe.currentLine = 5
        let store = makeStore(probe)
        store.apply(.promptStart(attrs: [:]))
        XCTAssertEqual(store.blocks.count, 1)
        XCTAssertEqual(store.blocks[0].startLine, 5)
        XCTAssertTrue(store.blocks[0].isRunning)
        XCTAssertNil(store.blocks[0].endLine)
        XCTAssertNil(store.blocks[0].frozenSnapshot)
    }

    func testOutputStartFillsCommand() {
        let store = makeStore(Probe())
        store.apply(.promptStart(attrs: [:]))
        // `cmd=` arrives base64-encoded — "bHM=" is "ls".
        store.apply(.outputStart(attrs: ["cmd": "bHM="]))
        XCTAssertEqual(store.blocks.first?.command, "ls")
    }

    func testCommandEndSealsAndFreezes() throws {
        let probe = Probe()
        probe.currentLine = 3
        let store = makeStore(probe)
        store.apply(.promptStart(attrs: [:]))
        store.apply(.outputStart(attrs: ["cmd": "bHM="]))
        // Cursor advanced as the command produced output.
        probe.currentLine = 7
        store.apply(.commandEnd(exitCode: 0, attrs: ["dur": "1234"]))
        let block = try XCTUnwrap(store.blocks.first)
        XCTAssertFalse(block.isRunning)
        XCTAssertEqual(block.endLine, 8)  // currentLine + 1
        XCTAssertEqual(block.exitCode, 0)
        XCTAssertEqual(try XCTUnwrap(block.duration), 1.234, accuracy: 0.001)
        XCTAssertNotNil(block.frozenSnapshot)
        // Snapshot range covers [startLine, endLine).
        XCTAssertEqual(probe.snapshotCalls.last?.start, 3)
        XCTAssertEqual(probe.snapshotCalls.last?.end, 8)
    }

    func testCwdAttrParsedFromPromptStart() {
        // base64 "/home/alice" = "L2hvbWUvYWxpY2U="
        let store = makeStore(Probe())
        store.apply(.promptStart(attrs: ["cwd": "L2hvbWUvYWxpY2U="]))
        XCTAssertEqual(store.blocks.first?.workingDirectory, "/home/alice")
    }

    func testNonzeroExitCarried() {
        let store = makeStore(Probe())
        store.apply(.promptStart(attrs: [:]))
        store.apply(.outputStart(attrs: [:]))
        store.apply(.commandEnd(exitCode: 127, attrs: [:]))
        XCTAssertEqual(store.blocks.first?.exitCode, 127)
    }

    func testMissingExitAndDur() {
        let store = makeStore(Probe())
        store.apply(.promptStart(attrs: [:]))
        store.apply(.outputStart(attrs: [:]))
        store.apply(.commandEnd(exitCode: nil, attrs: [:]))
        XCTAssertNil(store.blocks.first?.exitCode)
        XCTAssertNil(store.blocks.first?.duration)
    }

    // MARK: - Edge cases

    func testNewPromptSealsPreviousIfRunning() {
        // The remote shell can emit `A` again without a closing `D` if the
        // user Ctrl-C'd, or if the integration is partial. We seal the
        // running block with `exitCode = nil`.
        let probe = Probe()
        probe.currentLine = 1
        let store = makeStore(probe)
        store.apply(.promptStart(attrs: [:]))
        probe.currentLine = 4
        store.apply(.promptStart(attrs: [:]))
        XCTAssertEqual(store.blocks.count, 2)
        XCTAssertFalse(store.blocks[0].isRunning)
        XCTAssertNil(store.blocks[0].exitCode)
        XCTAssertEqual(store.blocks[1].startLine, 4)
    }

    func testOutputStartOpensBlockIfMissingPromptStart() {
        // Reconnect race: an `OSC 133 ; C` arrives without a preceding
        // `; A`. We open a block synthetically so output isn't dropped.
        let probe = Probe()
        probe.currentLine = 9
        let store = makeStore(probe)
        store.apply(.outputStart(attrs: ["cmd": "ZGF0ZQ=="]))  // "date"
        XCTAssertEqual(store.blocks.count, 1)
        XCTAssertEqual(store.blocks.first?.command, "date")
    }

    func testResetClearsAll() {
        let store = makeStore(Probe())
        store.apply(.promptStart(attrs: [:]))
        store.apply(.outputStart(attrs: [:]))
        store.apply(.commandEnd(exitCode: 0, attrs: [:]))
        XCTAssertFalse(store.blocks.isEmpty)
        store.reset()
        XCTAssertTrue(store.blocks.isEmpty)
    }

    func testCommandStartIsInformationalOnly() {
        // 133;B doesn't open or seal anything in our state machine; the
        // block must already exist from a preceding `A`, and the only
        // observable effect is that state stays consistent.
        let store = makeStore(Probe())
        store.apply(.promptStart(attrs: [:]))
        let countBefore = store.blocks.count
        store.apply(.commandStart(attrs: [:]))
        XCTAssertEqual(store.blocks.count, countBefore)
        XCTAssertTrue(store.blocks[0].isRunning)
    }
}
