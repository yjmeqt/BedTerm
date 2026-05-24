import SwiftUI
import UIKit

struct TerminalMetalHostView: UIViewRepresentable {
    let session: TerminalSession
    let feed: AsyncStream<Data>
    let onSend: (Data) -> Void
    let onResize: (Int, Int) -> Void
    let focusHandle: FocusHandle
    let yieldFirstResponder: Bool
    let displayMode: TerminalDisplayMode
    /// Called once when the MTKView is created, so the block-list overlay
    /// can push layout data to the shared render surface.
    let onMetalView: (TerminalMetalUIView) -> Void

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
        view.displayMode = displayMode
        focusHandle.bind(view)
        if !yieldFirstResponder {
            _ = view.becomeFirstResponder()
        }
        onMetalView(view)
        return view
    }

    func updateUIView(_ uiView: TerminalMetalUIView, context: Context) {
        uiView.displayMode = displayMode
        if yieldFirstResponder, uiView.isFirstResponder {
            _ = uiView.resignFirstResponder()
        } else if !yieldFirstResponder, !uiView.isFirstResponder {
            _ = uiView.becomeFirstResponder()
        }
    }
}
