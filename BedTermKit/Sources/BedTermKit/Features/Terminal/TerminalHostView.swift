import SwiftTerm
import SwiftUI
import UIKit

/// Wraps SwiftTerm's UIKit `TerminalView` for SwiftUI.
/// - `feed`: bytes coming from the SSH session, fed into the parser.
/// - `onSend`: bytes typed by the user (system keyboard) — forwarded to the SSH session.
/// - `onResize`: cols/rows after a layout pass — forwarded so the remote PTY can resize.
struct TerminalHostView: UIViewRepresentable {
    let feed: AsyncStream<Data>
    let onSend: (Data) -> Void
    let onResize: (Int, Int) -> Void
    /// Bound on `makeUIView`. Callers can ask whether the remote has enabled
    /// bracketed paste mode (CSI ? 2004 h). Returns `false` until the view exists.
    let bracketedPasteProbe: BracketedPasteProbe
    /// When true, the host yields first-responder so a SwiftUI `TextEditor`
    /// can capture the system keyboard. When false, the host claims first-
    /// responder so keystrokes pass through to the PTY.
    var yieldFirstResponder: Bool = false

    final class BracketedPasteProbe {
        private weak var view: SwiftTerm.TerminalView?
        func bind(_ view: SwiftTerm.TerminalView) { self.view = view }
        @MainActor func isActive() -> Bool {
            view?.getTerminal().bracketedPasteMode ?? false
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onSend: onSend, onResize: onResize)
    }

    func makeUIView(context: Context) -> SwiftTerm.TerminalView {
        let view = SwiftTerm.TerminalView()
        bracketedPasteProbe.bind(view)
        view.terminalDelegate = context.coordinator
        view.inputAccessoryView = nil
        // SwiftTerm's default CoreGraphics renderer ignores contentOffset when the
        // scrollback buffer exceeds the viewport, which leaves stale glyphs in the
        // backing store as the user scrolls (see prd:bedterm/mvp#bug.scroll_drawing_ghosting).
        // The Metal renderer honors contentOffset via metalVisibleRange() and repaints
        // on every scroll tick, so opt into it whenever the device supports Metal.
        // Trade-off: ANSI colours render flattened on the Metal path
        // (prd:bedterm/mvp#bug.ansi_colors_lost_with_metal). A readable monochrome
        // terminal beats a colourful but garbled one.
        try? view.setUseMetal(true)
        context.coordinator.start(consuming: feed, view: view)
        if !yieldFirstResponder {
            _ = view.becomeFirstResponder()
        }
        return view
    }

    func updateUIView(_ uiView: SwiftTerm.TerminalView, context: Context) {
        if yieldFirstResponder {
            if uiView.isFirstResponder { _ = uiView.resignFirstResponder() }
        } else {
            if !uiView.isFirstResponder { _ = uiView.becomeFirstResponder() }
        }
    }

    final class Coordinator: NSObject, TerminalViewDelegate {
        private let onSend: (Data) -> Void
        private let onResize: (Int, Int) -> Void
        private var consumeTask: Task<Void, Never>?
        private var anchorTimer: Task<Void, Never>?
        private var didRequestInitialAnchor = false

        init(onSend: @escaping (Data) -> Void, onResize: @escaping (Int, Int) -> Void) {
            self.onSend = onSend
            self.onResize = onResize
        }

        func start(consuming feed: AsyncStream<Data>, view: SwiftTerm.TerminalView) {
            consumeTask?.cancel()
            consumeTask = Task { @MainActor [weak view, weak self] in
                for await chunk in feed {
                    guard let view, let self else { return }
                    let hadScreenClear = Self.containsScreenClear(chunk)
                    view.feed(byteArray: ArraySlice(chunk))
                    if hadScreenClear {
                        self.scheduleBottomAnchorPass(view: view)
                    }
                }
            }
        }

        // MARK: TerminalViewDelegate

        func send(source: SwiftTerm.TerminalView, data: ArraySlice<UInt8>) {
            onSend(Data(data))
        }

        func sizeChanged(source: SwiftTerm.TerminalView, newCols: Int, newRows: Int) {
            onResize(newCols, newRows)
            // The first time SwiftTerm reports a real row count, schedule a bottom-anchor
            // pass so the shell's initial banner + prompt land at the bottom of the
            // viewport instead of the top.
            if !didRequestInitialAnchor, newRows > 3 {
                didRequestInitialAnchor = true
                scheduleBottomAnchorPass(view: source)
            }
        }

        func scrolled(source: SwiftTerm.TerminalView, position: Double) {}

        func setTerminalTitle(source: SwiftTerm.TerminalView, title: String) {}

        func hostCurrentDirectoryUpdate(source: SwiftTerm.TerminalView, directory: String?) {}

        func requestOpenLink(source: SwiftTerm.TerminalView, link: String, params: [String: String]) {}

        func bell(source: SwiftTerm.TerminalView) {}

        func clipboardCopy(source: SwiftTerm.TerminalView, content: Data) {
            UIPasteboard.general.string = String(data: content, encoding: .utf8)
        }

        func iTermContent(source: SwiftTerm.TerminalView, content: ArraySlice<UInt8>) {}

        func rangeChanged(source: SwiftTerm.TerminalView, startY: Int, endY: Int) {}

        deinit {
            consumeTask?.cancel()
            anchorTimer?.cancel()
        }

        // MARK: Bottom anchoring

        // Standard "clear screen" CSI sequences emitted by `clear`, Ctrl+L (readline),
        // `tput clear`, `reset`, and the `ESC c` RIS code. Matching is conservative — we
        // only bottom-anchor after a full erase + a quiet period, so TUI apps like vim
        // that clear before drawing their own UI are not disrupted.
        private static let screenClearPatterns: [[UInt8]] = [
            Array("\u{1B}[2J".utf8),
            Array("\u{1B}[3J".utf8),
            [0x1B, 0x63]
        ]

        private static func containsScreenClear(_ chunk: Data) -> Bool {
            guard !chunk.isEmpty else { return false }
            let bytes = [UInt8](chunk)
            for pattern in screenClearPatterns where indexOfSubsequence(of: pattern, in: bytes) != nil {
                return true
            }
            return false
        }

        private static func indexOfSubsequence(of needle: [UInt8], in haystack: [UInt8]) -> Int? {
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

        private func scheduleBottomAnchorPass(view: SwiftTerm.TerminalView) {
            anchorTimer?.cancel()
            anchorTimer = Task { @MainActor [weak view, weak self] in
                // Give the shell ~120 ms to finish emitting its new prompt before we decide.
                // TUI apps keep streaming during this window, which keeps the post-flight
                // emptiness check below failing and the anchor pass a no-op.
                try? await Task.sleep(nanoseconds: 120_000_000)
                if Task.isCancelled { return }
                guard let view, let self else { return }
                self.applyBottomAnchorIfShellAtTop(view: view)
            }
        }

        private func applyBottomAnchorIfShellAtTop(view: SwiftTerm.TerminalView) {
            let terminal = view.getTerminal()
            let rows = terminal.rows
            let (col, row) = terminal.getCursorLocation()
            guard rows > 3, row >= 0, row < rows / 2 else { return }
            // Only anchor when every visible row strictly below the cursor is blank.
            // If a TUI program (vim, top, less, fzf) has drawn its UI, those rows are
            // populated and we leave the layout alone.
            for rowIndex in (row + 1)..<rows {
                if let line = terminal.getLine(row: rowIndex), line.hasAnyContent() {
                    return
                }
            }
            let linesToInsert = rows - 1 - row
            guard linesToInsert > 0 else { return }
            // Move to home, insert N blank lines (which pushes the existing prompt row
            // down to the bottom), then re-park the cursor on the same column of the
            // new bottom row so future shell output lands where the user expects.
            let sequence = "\u{1B}[1;1H\u{1B}[\(linesToInsert)L\u{1B}[\(rows);\(col + 1)H"
            view.feed(byteArray: ArraySlice([UInt8](sequence.utf8)))
        }
    }
}
