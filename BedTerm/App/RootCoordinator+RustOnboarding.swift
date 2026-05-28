import BedTermIOS
import BedTermKit
import UIKit

/// W23d — Rust now owns the full onboarding flow (state machine + step
/// transitions + completion persistence). The Swift side only constructs
/// the Rust-built `BtIosOnboardingFlowVC` and routes its single
/// `on_completed` callback into `RootCoordinator.finishOnboarding()`.
extension RootCoordinator {
    /// Heap-boxed completion handler retained while the flow VC is alive.
    /// Released from the C trampoline once `on_completed` has fired.
    final class OnboardingFlowCompletionBox {
        let fire: () -> Void
        init(fire: @escaping () -> Void) { self.fire = fire }
    }

    func makeOnboardingFlowVC() -> UIViewController {
        let box = OnboardingFlowCompletionBox { [weak self] in
            self?.finishOnboarding()
        }
        // `passRetained` — the box is released from the C trampoline after
        // fire() runs. If the VC is destroyed before the user finishes
        // (e.g. UI tests tear down without completing onboarding), the
        // box leaks at most once per app session.
        let ctx = Unmanaged.passRetained(box).toOpaque()
        let callback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
            guard let ctx else { return }
            let box = Unmanaged<OnboardingFlowCompletionBox>.fromOpaque(ctx).takeRetainedValue()
            box.fire()
        }
        guard let raw = bt_ios_create_onboarding_flow_vc(callback, ctx) else {
            Unmanaged<OnboardingFlowCompletionBox>.fromOpaque(ctx).release()
            return UIViewController()
        }
        return Unmanaged<UIViewController>.fromOpaque(raw).takeRetainedValue()
    }
}
