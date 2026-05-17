import SwiftUI
import UIKit

struct TerminalMetalHostView: UIViewRepresentable {
    let feed: AsyncStream<Data>
    let onSend: (Data) -> Void
    let onResize: (Int, Int) -> Void
    let yieldFirstResponder: Bool

    func makeUIView(context: Context) -> TerminalMetalUIView {
        TerminalMetalUIView(feed: feed, onSend: onSend, onResize: onResize)
    }

    func updateUIView(_ uiView: TerminalMetalUIView, context: Context) {
        if yieldFirstResponder, uiView.isFirstResponder {
            _ = uiView.resignFirstResponder()
        } else if !yieldFirstResponder, !uiView.isFirstResponder {
            _ = uiView.becomeFirstResponder()
        }
    }
}
