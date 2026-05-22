import Foundation
import Testing

@testable import BedTermKit

@Suite("TerminalCore — GridSnapshot")
struct TerminalCoreTests {
    @Test("GridSnapshot cell accessor returns expected cells")
    func gridSnapshotCellAccessor() {
        let cells = (0..<20).map { idx in
            GridSnapshot.Cell(ch: UInt32(0x41 + (idx % 26)), fgRGBA: 0xFFFFFFFF, bgRGBA: 0x000000FF, flags: 0)
        }
        let snap = GridSnapshot(cols: 5, rows: 4, cursorCol: 0, cursorRow: 0, cells: cells)
        #expect(snap.cell(col: 0, row: 0)?.ch == UInt32(0x41))
        #expect(snap.cell(col: 4, row: 3)?.ch == UInt32(0x41 + 19 % 26))
        #expect(snap.cell(col: 5, row: 0) == nil)
    }
}

@Suite("TerminalCore — FFI")
struct TerminalCoreFFITests {
    @Test("new + feed ASCII")
    func newAndFeedASCII() {
        let core = TerminalCore(cols: 20, rows: 5)
        core.feed(Data("hi".utf8))
        let snap = core.snapshot()
        #expect(snap.cols == 20)
        #expect(snap.rows == 5)
        #expect(snap.cell(col: 0, row: 0)?.ch == UInt32(0x68))  // 'h'
        #expect(snap.cell(col: 1, row: 0)?.ch == UInt32(0x69))  // 'i'
        #expect(snap.cursorCol == 2)
        #expect(snap.cursorRow == 0)
    }

    @Test("alt-screen mode bit toggles via DEC mode 1049")
    func altScreenModeBit() {
        let core = TerminalCore(cols: 80, rows: 24)
        #expect(!core.mode.contains(.altScreen))
        // ESC[?1049h enters alt-screen (DEC mode 1049, used by vim/htop/etc).
        core.feed(Data([0x1B, 0x5B, 0x3F, 0x31, 0x30, 0x34, 0x39, 0x68]))
        #expect(core.mode.contains(.altScreen))
        // ESC[?1049l leaves it.
        core.feed(Data([0x1B, 0x5B, 0x3F, 0x31, 0x30, 0x34, 0x39, 0x6C]))
        #expect(!core.mode.contains(.altScreen))
    }

    @Test("bracketed-paste mode bit toggles via DEC mode 2004")
    func bracketedPasteModeBit() {
        let core = TerminalCore(cols: 80, rows: 24)
        #expect(!core.mode.contains(.bracketedPaste))
        // ESC[?2004h enables bracketed paste.
        core.feed(Data([0x1B, 0x5B, 0x3F, 0x32, 0x30, 0x30, 0x34, 0x68]))
        #expect(core.mode.contains(.bracketedPaste))
    }

    @Test("resize updates dimensions")
    func resizeUpdatesDimensions() {
        let core = TerminalCore(cols: 20, rows: 5)
        core.resize(cols: 40, rows: 10)
        #expect(core.snapshot().cols == 40)
        #expect(core.snapshot().rows == 10)
    }

    @Test("ANSI red applies to foreground")
    func ansiRedAppliesToFg() throws {
        let core = TerminalCore(cols: 20, rows: 5)
        core.feed(Data("\u{1B}[31mR\u{1B}[0m".utf8))
        let cell = try #require(core.snapshot().cell(col: 0, row: 0))
        #expect(cell.ch == UInt32(0x52))  // 'R'
        let red = (cell.fgRGBA >> 24) & 0xff
        let green = (cell.fgRGBA >> 16) & 0xff
        let blue = (cell.fgRGBA >> 8) & 0xff
        #expect(red > 0x80)
        #expect(green < 0x40)
        #expect(blue < 0x40)
    }
}
