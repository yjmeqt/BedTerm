import XCTest

@testable import BedTermKit

final class SelectionGeometryTests: XCTestCase {
    let cell = CGSize(width: 10, height: 20)

    func testEmptyRange() {
        let range = SelectionRange(startRow: 2, startCol: 3, endRow: 2, endCol: 3)
        XCTAssertTrue(range.rects(cellSize: cell, cols: 80).isEmpty)
    }

    func testSingleRow() {
        let range = SelectionRange(startRow: 1, startCol: 2, endRow: 1, endCol: 5)
        let rects = range.rects(cellSize: cell, cols: 80)
        XCTAssertEqual(rects, [CGRect(x: 20, y: 20, width: 30, height: 20)])
    }

    func testMultiRow() {
        let range = SelectionRange(startRow: 0, startCol: 70, endRow: 2, endCol: 3)
        let rects = range.rects(cellSize: cell, cols: 80)
        XCTAssertEqual(rects.count, 3)
        XCTAssertEqual(rects[0], CGRect(x: 700, y: 0, width: 100, height: 20))
        XCTAssertEqual(rects[1], CGRect(x: 0, y: 20, width: 800, height: 20))
        XCTAssertEqual(rects[2], CGRect(x: 0, y: 40, width: 30, height: 20))
    }

    func testBackwardsNormalises() {
        let range = SelectionRange(startRow: 2, startCol: 3, endRow: 1, endCol: 5)
        let norm = range.normalised
        XCTAssertEqual(norm.startRow, 1)
        XCTAssertEqual(norm.startCol, 5)
        XCTAssertEqual(norm.endRow, 2)
        XCTAssertEqual(norm.endCol, 3)
    }
}
