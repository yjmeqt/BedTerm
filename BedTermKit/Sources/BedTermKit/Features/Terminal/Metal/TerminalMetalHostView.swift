import SwiftUI
import UIKit

struct TerminalMetalHostView: UIViewRepresentable {
    let session: TerminalSession
    let feed: AsyncStream<Data>
    let onSend: (Data) -> Void
    let onResize: (Int, Int) -> Void
    let focusHandle: FocusHandle
    let yieldFirstResponder: Bool

    /// Lets callers outside the SwiftUI view tree (e.g. the composer) hand
    /// first-responder back to the terminal *before* their own view is torn
    /// down — keeping the system keyboard up across the handoff.
    public final class FocusHandle {
        private weak var view: UIView?
        func bind(_ view: UIView) { self.view = view }
        @MainActor public func claimFirstResponder() {
            guard let view, !view.isFirstResponder else { return }
            _ = view.becomeFirstResponder()
        }
    }

    func makeUIView(context: Context) -> TerminalMetalUIView {
        let view = TerminalMetalUIView(session: session, feed: feed, onSend: onSend, onResize: onResize)
        focusHandle.bind(view)
        if !yieldFirstResponder {
            _ = view.becomeFirstResponder()
        }
        return view
    }

    func updateUIView(_ uiView: TerminalMetalUIView, context: Context) {
        if yieldFirstResponder, uiView.isFirstResponder {
            _ = uiView.resignFirstResponder()
        } else if !yieldFirstResponder, !uiView.isFirstResponder {
            _ = uiView.becomeFirstResponder()
        }
    }
}
