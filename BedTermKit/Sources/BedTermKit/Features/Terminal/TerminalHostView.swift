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

    func makeCoordinator() -> Coordinator {
        Coordinator(onSend: onSend, onResize: onResize)
    }

    func makeUIView(context: Context) -> SwiftTerm.TerminalView {
        let view = SwiftTerm.TerminalView()
        view.terminalDelegate = context.coordinator
        context.coordinator.start(consuming: feed, view: view)
        _ = view.becomeFirstResponder()
        return view
    }

    func updateUIView(_ uiView: SwiftTerm.TerminalView, context: Context) {
        // SwiftTerm handles its own layout; nothing to push on update.
    }

    final class Coordinator: NSObject, TerminalViewDelegate {
        private let onSend: (Data) -> Void
        private let onResize: (Int, Int) -> Void
        private var consumeTask: Task<Void, Never>?

        init(onSend: @escaping (Data) -> Void, onResize: @escaping (Int, Int) -> Void) {
            self.onSend = onSend
            self.onResize = onResize
        }

        func start(consuming feed: AsyncStream<Data>, view: SwiftTerm.TerminalView) {
            consumeTask?.cancel()
            consumeTask = Task { @MainActor [weak view] in
                for await chunk in feed {
                    guard let view else { return }
                    view.feed(byteArray: ArraySlice(chunk))
                }
            }
        }

        // MARK: TerminalViewDelegate

        func send(source: SwiftTerm.TerminalView, data: ArraySlice<UInt8>) {
            onSend(Data(data))
        }

        func sizeChanged(source: SwiftTerm.TerminalView, newCols: Int, newRows: Int) {
            onResize(newCols, newRows)
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

        deinit { consumeTask?.cancel() }
    }
}
