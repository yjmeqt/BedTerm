import Foundation

/// Immutable, value-typed copy of the Rust grid for a single frame.
public struct GridSnapshot: Sendable, Equatable {
    public struct Cell: Sendable, Equatable {
        public let ch: UInt32
        public let fgRGBA: UInt32
        public let bgRGBA: UInt32
        public let flags: UInt16

        public init(ch: UInt32, fgRGBA: UInt32, bgRGBA: UInt32, flags: UInt16) {
            self.ch = ch
            self.fgRGBA = fgRGBA
            self.bgRGBA = bgRGBA
            self.flags = flags
        }
    }

    public let cols: UInt16
    public let rows: UInt16
    public let cursorCol: UInt16
    /// Equals `rows` when the cursor is scrolled off-screen — callers should
    /// hide the cursor overlay in that case.
    public let cursorRow: UInt16
    /// 0 = at live bottom; positive = N rows into scrollback.
    public let displayOffset: UInt32
    public let cells: [Cell]

    public init(
        cols: UInt16, rows: UInt16, cursorCol: UInt16, cursorRow: UInt16,
        displayOffset: UInt32 = 0, cells: [Cell]
    ) {
        precondition(cells.count == Int(cols) * Int(rows), "cell count must equal cols*rows")
        self.cols = cols
        self.rows = rows
        self.cursorCol = cursorCol
        self.cursorRow = cursorRow
        self.displayOffset = displayOffset
        self.cells = cells
    }

    public func cell(col: Int, row: Int) -> Cell? {
        guard col >= 0, row >= 0, col < Int(cols), row < Int(rows) else { return nil }
        return cells[row * Int(cols) + col]
    }
}
