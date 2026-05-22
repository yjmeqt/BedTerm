import SwiftUI
import UIKit

/// Read-only Metal terminal view for killed-session replay. Mounts a
/// `TerminalMetalUIView` in `isInputDisabled` mode so the user can
/// scroll and copy text but cannot type into the terminal.
struct TerminalReplayHostView: UIViewRepresentable {
    let replayCore: TerminalCore

    func makeUIView(context: Context) -> TerminalMetalUIView {
        TerminalMetalUIView(replayCore: replayCore)
    }

    func updateUIView(_ uiView: TerminalMetalUIView, context: Context) {
        // Replay cores are immutable after construction — no updates needed.
    }
}
