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
