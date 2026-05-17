import SwiftUI
import UIKit

/// Tracks the software keyboard's overlap with the key window, so SwiftUI
/// views can avoid the keyboard manually instead of relying on the framework's
/// automatic avoidance — which intermittently fails to release after a
/// programmatic `resignFirstResponder` (bug r15_toolbar_stuck_midscreen).
///
/// `overlap` is the keyboard's intersection with the window, minus the
/// bottom safe-area inset (home-indicator strip). A view pairs
/// `.padding(.bottom, overlap)` with `.ignoresSafeArea(.keyboard, edges: .bottom)`
/// — in that order — to ride above the keyboard when shown and snap back to
/// the safe-area bottom when hidden.
@MainActor
@Observable
final class KeyboardLayoutObserver {
    private(set) var overlap: CGFloat = 0

    init() {
        let center = NotificationCenter.default
        center.addObserver(
            forName: UIResponder.keyboardWillChangeFrameNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            let endFrame = note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect
            MainActor.assumeIsolated { self?.update(endFrame: endFrame) }
        }
        center.addObserver(
            forName: UIResponder.keyboardWillHideNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.overlap = 0 }
        }
    }

    private func update(endFrame: CGRect?) {
        guard let endFrame, let window = Self.keyWindow else { return }
        let intersection = window.bounds.intersection(endFrame)
        overlap = max(0, intersection.height - window.safeAreaInsets.bottom)
    }

    private static var keyWindow: UIWindow? {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
    }
}
