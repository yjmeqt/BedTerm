import GameController
import Observation

/// Tracks whether a hardware keyboard is currently attached to the device.
/// Used by the R15 KeyBar to hide the dismiss-keyboard toggle when there is
/// no software keyboard to dismiss (per R15.hardware_keyboard_hides_toggle).
@MainActor
@Observable
final class HardwareKeyboardObserver {
    static let shared = HardwareKeyboardObserver()

    private(set) var isAttached: Bool = GCKeyboard.coalesced != nil

    private init() {
        let center = NotificationCenter.default
        center.addObserver(
            forName: .GCKeyboardDidConnect,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.isAttached = true }
        }
        center.addObserver(
            forName: .GCKeyboardDidDisconnect,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.isAttached = GCKeyboard.coalesced != nil
            }
        }
    }
}
