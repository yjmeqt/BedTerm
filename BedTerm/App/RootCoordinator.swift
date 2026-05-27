import BedTermCoreC
import BedTermKit
import SwiftUI
import UIKit

/// Owns the app's root window, root `UINavigationController`, the
/// onboarding/harness branches, and the top-level toaster overlay.
@MainActor
final class RootCoordinator {
    private let window: UIWindow
    private let toaster = Toaster()
    private var navigationController: UINavigationController?
    private var toasterHost: UIHostingController<AnyView>?
    private var onboardingDone: Bool = bt_ios_settings_onboarding_completed()

    /// Strong reference to the active hosts controller (W24d). Owns
    /// the Rust hosts list VC, the `HostsViewModel`, and the
    /// connect/swap/delete/mismatch chrome.
    private var hostsController: HostsConnectController?

    init(window: UIWindow) {
        self.window = window
    }

    func start() {
        if let fixture = UITestSupport.rustTerminalDirectFixture {
            installHarnessRoot(fixture: fixture)
        } else if !onboardingDone && !UITestSupport.skipOnboarding {
            installOnboardingRoot()
        } else {
            onboardingDone = true
            installHostsRoot()
        }
        installToasterOverlay()
        window.makeKeyAndVisible()
    }

    // MARK: - Roots

    private func installHarnessRoot(fixture: String) {
        // The harness uses a SwiftUI NavigationStack internally because
        // the existing UI tests assert on a11y identifiers nested under
        // its representable. We don't need a UINavigationController here.
        let view = RustTerminalUITestHarness(fixture: fixture)
            .environment(\.toaster, toaster)
        let host = UIHostingController(rootView: AnyView(view))
        window.rootViewController = host
    }

    private func installOnboardingRoot() {
        showOnboarding()
    }

    /// Install the Rust-backed onboarding flow VC at the window root.
    /// After W23d the entire R10 step machine — host-kind picker, location
    /// picker, optional macOS tutorial, optional Local Network permission
    /// terminator — lives inside `BtIosOnboardingFlowVC`. Swift only
    /// reacts to the single `on_completed` callback (see
    /// `RootCoordinator+RustOnboarding.swift`) by swapping to the hosts
    /// root.
    private func showOnboarding() {
        let flowVC = makeOnboardingFlowVC()
        // The Rust constructor returns a `UINavigationController`
        // subclass, but the Swift side treats it as an opaque
        // UIViewController. Cast back so the rest of the coordinator can
        // continue to reach for `navigationController` (e.g. for the
        // toaster overlay z-ordering) if it ever needs to.
        self.navigationController = flowVC as? UINavigationController
        window.rootViewController = flowVC
    }

    func finishOnboarding() {
        onboardingDone = true
        installHostsRoot()
    }

    /// Install the Rust-built hosts list as the nav root. Connect /
    /// swap / delete / mismatch orchestration lives in
    /// `HostsConnectController`, which owns the same `HostsViewModel`
    /// the old SwiftUI screens used.
    private func installHostsRoot() {
        let controller = HostsConnectController(
            toaster: toaster,
            onShowHostForm: { [weak self] id, connectOnSave, onFinish in
                self?.pushHostForm(id: id, connectOnSave: connectOnSave, onFinish: onFinish)
            },
            onShowTerminal: { [weak self] vc in
                self?.pushTerminal(vc)
            },
            onPopToHosts: { [weak self] in
                self?.popToHosts()
            },
            onShowSettings: { [weak self] in
                self?.presentSettings()
            }
        )
        self.hostsController = controller
        let nav = UINavigationController(rootViewController: controller.rootViewController)
        self.navigationController = nav
        if window.rootViewController != nil {
            UIView.transition(
                with: window,
                duration: 0.25,
                options: .transitionCrossDissolve,
                animations: { self.window.rootViewController = nav },
                completion: nil
            )
        } else {
            window.rootViewController = nav
        }
        controller.start()
    }

    // MARK: - Nav pushes

    /// Heap-boxed coordinator captured by the Rust connect-form VC's C
    /// callbacks so they can hop back into Swift on completion.
    private final class FormBox {
        weak var nav: UINavigationController?
        let connectOnSave: Bool
        let onFinish: (ConnectionFormOutcome) -> Void
        init(
            nav: UINavigationController?,
            connectOnSave: Bool,
            onFinish: @escaping (ConnectionFormOutcome) -> Void
        ) {
            self.nav = nav
            self.connectOnSave = connectOnSave
            self.onFinish = onFinish
        }
    }

    private typealias FormDoneCallback =
        @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?, Bool) -> Void

    private static let formBoxOnDone: FormDoneCallback = { ctx, idPtr, connectNow in
        guard let ctx, let idPtr else { return }
        let idString = String(cString: idPtr)
        guard let uuid = UUID(uuidString: idString) else { return }
        let ctxRaw = UInt(bitPattern: ctx)
        DispatchQueue.main.async {
            guard let restored = UnsafeMutableRawPointer(bitPattern: ctxRaw) else { return }
            let box = Unmanaged<FormBox>.fromOpaque(restored).takeUnretainedValue()
            let outcome: ConnectionFormOutcome
            if connectNow || box.connectOnSave {
                outcome = .savedAndConnect(uuid)
            } else {
                outcome = .saved(uuid)
            }
            box.onFinish(outcome)
            box.nav?.popViewController(animated: true)
        }
    }

    private static let formBoxOnCancel: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
        guard let ctx else { return }
        let ctxRaw = UInt(bitPattern: ctx)
        DispatchQueue.main.async {
            guard let restored = UnsafeMutableRawPointer(bitPattern: ctxRaw) else { return }
            let box = Unmanaged<FormBox>.fromOpaque(restored).takeUnretainedValue()
            box.onFinish(.cancelled)
            box.nav?.popViewController(animated: true)
        }
    }

    /// Push the Rust-built connect form. Persistence still routes
    /// through the Swift `ConnectionFormViewModel` via the
    /// `bt_swift_connect_form_*` bridge.
    private func pushHostForm(
        id: SavedHost.ID?,
        connectOnSave: Bool,
        onFinish: @escaping (ConnectionFormOutcome) -> Void
    ) {
        guard let nav = navigationController else { return }
        let box = FormBox(nav: nav, connectOnSave: connectOnSave, onFinish: onFinish)
        let ctx = Unmanaged.passRetained(box).toOpaque()

        // The Rust VC reads/writes the hosts store directly through the
        // Rust `hosts_store` and the `bt_swift_hosts_store_save_json`
        // thin callback — no bridge store needed.

        let raw: UnsafeMutableRawPointer?
        if let id {
            raw = id.uuidString.withCString { idPtr in
                bt_ios_create_connect_form_vc(
                    idPtr, connectOnSave, Self.formBoxOnDone, Self.formBoxOnCancel, ctx)
            }
        } else {
            raw = bt_ios_create_connect_form_vc(
                nil, connectOnSave, Self.formBoxOnDone, Self.formBoxOnCancel, ctx)
        }
        guard let raw else {
            Unmanaged<FormBox>.fromOpaque(ctx).release()
            return
        }
        let vc = Unmanaged<UIViewController>.fromOpaque(raw).takeRetainedValue()
        // Pin the box to the VC lifetime so the +1 retain is balanced
        // when the VC is dropped.
        objc_setAssociatedObject(vc, &Self.formBoxKey, box, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        Unmanaged<FormBox>.fromOpaque(ctx).release()
        nav.pushViewController(vc, animated: true)
    }

    private static var formBoxKey: UInt8 = 0

    private func pushTerminal(_ vc: UIViewController) {
        navigationController?.pushViewController(vc, animated: true)
    }

    private func popToHosts() {
        navigationController?.popToRootViewController(animated: true)
    }

    /// Build a UIKit modal nav stack wrapping the Rust-built
    /// `BtIosSettingsViewController` and present it from the topmost VC on
    /// the root nav. Calls into `bt_ios_create_settings_vc`; the done-tap
    /// C callback dismisses the modal.
    private func presentSettings() {
        guard let nav = navigationController else { return }
        let modalNav = makeRustSettingsModalNav()
        modalNav.modalPresentationStyle = .formSheet
        let presenter = nav.topViewController ?? nav
        presenter.present(modalNav, animated: true)
    }

    private func makeRustSettingsModalNav() -> UINavigationController {
        // Heap-box a closure that captures a weak ref to the modal nav so
        // the C callback can dismiss it. The box itself is kept alive by
        // the wrapping VC's strong ref to it via the boxed pointer; once
        // the VC is released (after dismissal completes), the box drops.
        final class DoneBox {
            weak var nav: UINavigationController?
            init() {}
        }
        let box = DoneBox()
        let ctx = Unmanaged.passRetained(box).toOpaque()
        let callback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
            guard let ctx else { return }
            let box = Unmanaged<DoneBox>.fromOpaque(ctx).takeUnretainedValue()
            DispatchQueue.main.async {
                box.nav?.dismiss(animated: true)
            }
        }
        guard let raw = bt_ios_create_settings_vc(callback, ctx) else {
            // Rust constructor failed — return an empty nav so the modal
            // presents (and can be dismissed) rather than silently failing.
            Unmanaged<DoneBox>.fromOpaque(ctx).release()
            return UINavigationController(rootViewController: UIViewController())
        }
        let vc = Unmanaged<UIViewController>.fromOpaque(raw).takeRetainedValue()
        let modalNav = UINavigationController(rootViewController: vc)
        box.nav = modalNav
        // Pin the DoneBox to the modal nav so it is released alongside it.
        objc_setAssociatedObject(modalNav, &Self.doneBoxKey, box, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        // Balance the +1 we passed to ctx — the box is now strongly
        // associated with `modalNav`, so we can release our retain.
        Unmanaged<DoneBox>.fromOpaque(ctx).release()
        return modalNav
    }

    private static var doneBoxKey: UInt8 = 0

    // MARK: - Toaster

    private func installToasterOverlay() {
        let view = ToasterOverlay()
            .environment(\.toaster, toaster)
        let host = UIHostingController(rootView: AnyView(view))
        host.view.backgroundColor = .clear
        // Sit above the root VC so toasts overlay the nav stack but
        // don't intercept touches outside their cards.
        host.view.translatesAutoresizingMaskIntoConstraints = false
        if let rootView = window.rootViewController?.view {
            window.addSubview(host.view)
            // Anchor relative to the window's safe-area; the overlay
            // itself handles internal padding.
            NSLayoutConstraint.activate([
                host.view.topAnchor.constraint(equalTo: window.safeAreaLayoutGuide.topAnchor),
                host.view.leadingAnchor.constraint(equalTo: window.leadingAnchor),
                host.view.trailingAnchor.constraint(equalTo: window.trailingAnchor),
                host.view.heightAnchor.constraint(equalToConstant: 200)
            ])
            _ = rootView
        }
        toasterHost = host
    }
}
