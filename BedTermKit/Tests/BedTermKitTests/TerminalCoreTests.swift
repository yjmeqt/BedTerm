import Foundation
import Testing

@testable import BedTermKit

@Suite("TerminalCore — FFI")
struct TerminalCoreFFITests {
    @Test("new caches screen dimensions")
    func newCachesScreenDimensions() {
        let core = TerminalCore(cols: 20, rows: 5)
        #expect(core.screenCols == 20)
        #expect(core.screenRows == 5)
    }

    @Test("resize updates cached dimensions")
    func resizeUpdatesDimensions() {
        let core = TerminalCore(cols: 20, rows: 5)
        core.resize(cols: 40, rows: 10)
        #expect(core.screenCols == 40)
        #expect(core.screenRows == 10)
    }

    @Test("feed advances the cursor (currentLine stays on first row)")
    func feedAdvancesCursor() {
        let core = TerminalCore(cols: 20, rows: 5)
        core.feed(Data("hi".utf8))
        // Two ASCII chars on the first row — cursor still on line 0.
        #expect(core.currentLine == 0)
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

    @Test("scrollToBottom resets scroll offset to zero")
    func scrollToBottomClampsOffset() {
        let core = TerminalCore(cols: 20, rows: 5)
        core.scrollToBottom()
        #expect(core.scrollOffset == 0)
    }
}
