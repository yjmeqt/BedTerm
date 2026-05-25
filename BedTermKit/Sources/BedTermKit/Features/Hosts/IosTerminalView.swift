import BedTermCoreC
import SwiftUI
import UIKit

/// Hosts the Rust-backed `BtIosTerminalViewController` in a SwiftUI navigation stack.
struct IosTerminalView: UIViewControllerRepresentable {
    let onBack: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(onBack: onBack) }

    func makeUIViewController(context: Context) -> UIViewController {
        context.coordinator.makeVC()
    }

    func updateUIViewController(_ uiViewController: UIViewController, context: Context) {}

    final class Coordinator {
        var onBack: () -> Void

        init(onBack: @escaping () -> Void) {
            self.onBack = onBack
        }

        @MainActor func makeVC() -> UIViewController {
            // Pass a raw pointer to self as the callback context.
            let ctx = Unmanaged.passRetained(self).toOpaque()
            let ptr = bt_ios_create_vc(
                { rawCtx in
                    guard let rawCtx else { return }
                    let coordinator = Unmanaged<Coordinator>.fromOpaque(rawCtx)
                        .takeUnretainedValue()
                    coordinator.onBack()
                }, ctx)
            guard let ptr else {
                // Fallback: plain UIViewController if Rust returned null.
                return UIViewController()
            }
            return Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        }
    }
}
