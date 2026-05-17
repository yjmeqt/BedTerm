import XCTest

@testable import BedTermKit

final class TerminalCoreTests: XCTestCase {
    func testGridSnapshotCellAccessor() {
        let cells = (0..<20).map { idx in
            GridSnapshot.Cell(ch: UInt32(0x41 + (idx % 26)), fgRGBA: 0xFFFFFFFF, bgRGBA: 0x000000FF, flags: 0)
        }
        let snap = GridSnapshot(cols: 5, rows: 4, cursorCol: 0, cursorRow: 0, cells: cells)
        XCTAssertEqual(snap.cell(col: 0, row: 0)?.ch, UInt32(0x41))
        XCTAssertEqual(snap.cell(col: 4, row: 3)?.ch, UInt32(0x41 + 19 % 26))
        XCTAssertNil(snap.cell(col: 5, row: 0))
    }
}

final class TerminalCoreFFITests: XCTestCase {
    func testNewAndFeedASCII() {
        let core = TerminalCore(cols: 20, rows: 5)
        core.feed(Data("hi".utf8))
        let snap = core.snapshot()
        XCTAssertEqual(snap.cols, 20)
        XCTAssertEqual(snap.rows, 5)
        XCTAssertEqual(snap.cell(col: 0, row: 0)?.ch, UInt32(0x68))  // 'h'
        XCTAssertEqual(snap.cell(col: 1, row: 0)?.ch, UInt32(0x69))  // 'i'
        XCTAssertEqual(snap.cursorCol, 2)
        XCTAssertEqual(snap.cursorRow, 0)
    }

    func testResizeUpdatesDimensions() {
        let core = TerminalCore(cols: 20, rows: 5)
        core.resize(cols: 40, rows: 10)
        XCTAssertEqual(core.snapshot().cols, 40)
        XCTAssertEqual(core.snapshot().rows, 10)
    }

    func testANSIRedAppliesToFg() throws {
        let core = TerminalCore(cols: 20, rows: 5)
        core.feed(Data("\u{1B}[31mR\u{1B}[0m".utf8))
        let cell = try XCTUnwrap(core.snapshot().cell(col: 0, row: 0))
        XCTAssertEqual(cell.ch, UInt32(0x52))  // 'R'
        let red = (cell.fgRGBA >> 24) & 0xff
        let green = (cell.fgRGBA >> 16) & 0xff
        let blue = (cell.fgRGBA >> 8) & 0xff
        XCTAssertGreaterThan(red, 0x80)
        XCTAssertLessThan(green, 0x40)
        XCTAssertLessThan(blue, 0x40)
    }
}
