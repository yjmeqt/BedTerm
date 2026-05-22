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
    /// Mirrors the system keyboard's actual visibility, derived from
    /// `keyboardWillShow` / `keyboardWillHide`. Flips at the start of
    /// each transition (not the end), so views that react to "the
    /// keyboard is going away" — e.g. the R6 dismiss chevron icon —
    /// settle in lockstep with the keyboard's own slide. Sourced from
    /// the OS, so it stays correct regardless of *why* the keyboard
    /// moved (toggle tap, drag-down dismiss, focus change, app
    /// background, hardware-keyboard attach, …).
    private(set) var isHidden: Bool = true

    init() {
        let center = NotificationCenter.default
        center.addObserver(
            forName: UIResponder.keyboardWillChangeFrameNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            // Pull Sendable values off the notification before the actor hop —
            // Notification itself isn't Sendable under Swift 6, and passing
            // it to a helper function counts as "sending" across isolation.
            let endFrame = note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect
            let duration =
                (note.userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double) ?? 0.25
            MainActor.assumeIsolated { self?.update(endFrame: endFrame, duration: duration) }
        }
        center.addObserver(
            forName: UIResponder.keyboardWillShowNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.isHidden = false }
        }
        center.addObserver(
            forName: UIResponder.keyboardWillHideNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            let duration =
                (note.userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double) ?? 0.25
            MainActor.assumeIsolated {
                self?.isHidden = true
                self?.write(0, duration: duration)
            }
        }
    }

    private func update(endFrame: CGRect?, duration: Double) {
        guard let endFrame, let window = Self.keyWindow else { return }
        let intersection = window.bounds.intersection(endFrame)
        let value = max(0, intersection.height - window.safeAreaInsets.bottom)
        if value > 0 { isHidden = false }
        write(value, duration: duration)
    }

    private func write(_ value: CGFloat, duration: Double) {
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
