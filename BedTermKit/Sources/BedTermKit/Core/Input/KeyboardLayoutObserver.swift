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
///
/// Writes to `overlap` are wrapped in `withAnimation` using the keyboard's
/// own animation duration (carried in the notification's userInfo). Callers
/// must NOT add a separate `.animation(value: overlap)` modifier — doing so
/// would run a second, mismatched curve on top of the keyboard's own and
/// cause a visible jump as the two timings drift apart.
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
            MainActor.assumeIsolated { self?.update(from: note) }
        }
        center.addObserver(
            forName: UIResponder.keyboardWillHideNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            MainActor.assumeIsolated { self?.write(0, using: note) }
        }
    }

    private func update(from note: Notification) {
        guard let userInfo = note.userInfo,
            let endFrame = userInfo[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect,
            let window = Self.keyWindow
        else { return }
        let intersection = window.bounds.intersection(endFrame)
        let value = max(0, intersection.height - window.safeAreaInsets.bottom)
        write(value, using: note)
    }

    private func write(_ value: CGFloat, using note: Notification) {
        let duration =
            (note.userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double) ?? 0.25
        withAnimation(.smooth(duration: duration)) {
            overlap = value
        }
    }

    private static var keyWindow: UIWindow? {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
    }
}
