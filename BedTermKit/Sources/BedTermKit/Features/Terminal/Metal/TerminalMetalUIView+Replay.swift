import MetalKit

// MARK: - Bottom anchoring
//
// Screen-clear detection and the prompt-bottom-anchor pass live here so
// the main TerminalMetalUIView.swift stays within the file-length limit.

extension TerminalMetalUIView {
    // Standard "clear screen" CSI sequences emitted by `clear`, Ctrl+L,
    // `tput clear`, `reset`, and the `ESC c` RIS code. We bottom-anchor
    // only after one of these + a quiet period, so TUI apps that clear
    // before drawing their own UI are not disrupted.
    static let screenClearPatterns: [[UInt8]] = [
        Array("\u{1B}[2J".utf8),
        Array("\u{1B}[3J".utf8),
        [0x1B, 0x63]
    ]

    static func containsScreenClear(_ chunk: Data) -> Bool {
        guard !chunk.isEmpty else { return false }
        let bytes = [UInt8](chunk)
        for pattern in screenClearPatterns where indexOfSubsequence(of: pattern, in: bytes) != nil {
            return true
        }
        return false
    }

    static func indexOfSubsequence(of needle: [UInt8], in haystack: [UInt8]) -> Int? {
        guard !needle.isEmpty, haystack.count >= needle.count else { return nil }
        let last = haystack.count - needle.count
        for offset in 0...last {
            var match = true
            for pos in 0..<needle.count where haystack[offset + pos] != needle[pos] {
                match = false
                break
            }
            if match { return offset }
        }
        return nil
    }

    func scheduleBottomAnchorPass() {
        anchorTask?.cancel()
        anchorTask = Task { @MainActor [weak self] in
            // Give the shell ~120 ms to finish emitting its new prompt before
            // we decide. TUI apps keep streaming during this window, which
            // keeps the post-flight emptiness check below failing and the
            // anchor pass a no-op.
            try? await Task.sleep(nanoseconds: 120_000_000)
            if Task.isCancelled { return }
            self?.applyBottomAnchorIfShellAtTop()
        }
    }

    func applyBottomAnchorIfShellAtTop() {
        let snapshot = terminalCore.snapshot()
        let rows = Int(snapshot.rows)
        let cols = Int(snapshot.cols)
        let row = Int(snapshot.cursorRow)
        let col = Int(snapshot.cursorCol)
        guard rows > 3, row >= 0, row < rows / 2 else { return }
        // Only anchor when every visible row strictly below the cursor is blank.
        for rowIndex in (row + 1)..<rows {
            for col in 0..<cols {
                if let cell = snapshot.cell(col: col, row: rowIndex), cell.ch != 0 {
                    return
                }
            }
        }
        let linesToInsert = rows - 1 - row
        guard linesToInsert > 0 else { return }
        // Move to home, insert N blank lines (which pushes the existing prompt
        // row down to the bottom), then re-park the cursor on the same column
        // of the new bottom row.
        let sequence = "\u{1B}[1;1H\u{1B}[\(linesToInsert)L\u{1B}[\(rows);\(col + 1)H"
        terminalCore.feed(Data(sequence.utf8))
        setNeedsDisplay()
    }
}
