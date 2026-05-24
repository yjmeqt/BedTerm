import Foundation
import Testing

@testable import BedTermKit

@Suite("SelectionGeometry")
struct SelectionGeometryTests {
    let cell = CGSize(width: 10, height: 20)

    @Test("empty range produces no rects")
    func emptyRange() {
        let range = SelectionRange(startRow: 2, startCol: 3, endRow: 2, endCol: 3)
        #expect(range.rects(cellSize: cell, cols: 80).isEmpty)
    }

    @Test("single-row range produces one rect")
    func singleRow() {
        let range = SelectionRange(startRow: 1, startCol: 2, endRow: 1, endCol: 5)
        let rects = range.rects(cellSize: cell, cols: 80)
        #expect(rects == [CGRect(x: 20, y: 20, width: 30, height: 20)])
    }

    @Test("multi-row range produces three rects")
    func multiRow() {
        let range = SelectionRange(startRow: 0, startCol: 70, endRow: 2, endCol: 3)
        let rects = range.rects(cellSize: cell, cols: 80)
        #expect(rects.count == 3)
        #expect(rects[0] == CGRect(x: 700, y: 0, width: 100, height: 20))
        #expect(rects[1] == CGRect(x: 0, y: 20, width: 800, height: 20))
        #expect(rects[2] == CGRect(x: 0, y: 40, width: 30, height: 20))
    }

    @Test("backwards range normalises start/end")
    func backwardsNormalises() {
        let range = SelectionRange(startRow: 2, startCol: 3, endRow: 1, endCol: 5)
        let norm = range.normalised
        #expect(norm.startRow == 1)
        #expect(norm.startCol == 5)
        #expect(norm.endRow == 2)
        #expect(norm.endCol == 3)
    }
}
