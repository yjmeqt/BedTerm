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
    private let settings = BedTermSettings()
    private var navigationController: UINavigationController?
    private var toasterHost: UIHostingController<AnyView>?
    private var onboardingDone: Bool = OnboardingPersistenceBridge.hasCompleted

    init(window: UIWindow) {
        self.window = window
        // Install the shared settings store handle so the Rust Settings
        // VC (W23b) can round-trip values through the `bt_swift_settings_*`
        // C ABI. Done at coordinator-init time, before any Rust VC can be
        // constructed downstream.
        SettingsBridge.observableHandle = settings
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
            .environment(settings)
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

    private func installHostsRoot() {
        if settings.useRustHostsList {
            installRustHostsRoot()
            return
        }
        installSwiftUIHostsRoot()
    }

    /// SwiftUI path (default) — install the legacy `HostsScreen`. Kept
    /// behind a tiny fork so the W24d flag-flip retirement diff stays
    /// trivial.
    private func installSwiftUIHostsRoot() {
        let hosts = HostsScreen(
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
        .environment(\.toaster, toaster)
        .environment(settings)
        let hostsVC = UIHostingController(rootView: AnyView(hosts))
        let nav = UINavigationController(rootViewController: hostsVC)
        self.navigationController = nav
        // Animated swap if we're switching from onboarding; otherwise
        // first install.
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
    }

    // MARK: - Rust hosts root (W24b)

    /// W24b phase 1: install the Rust-built hosts list as the nav root.
    /// The Rust VC renders rows + the `+` button; connect / delete /
    /// mismatch orchestration stays on the SwiftUI side, driven by
    /// `RustHostsScreen` which wraps the Rust VC in a
    /// `UIViewControllerRepresentable` and reuses `HostsViewModel`
    /// end-to-end. Once W24d flips the default and removes
    /// `HostsScreen`, the SwiftUI fork collapses to this path.
    private func installRustHostsRoot() {
        let screen = RustHostsScreen(
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
        .environment(\.toaster, toaster)
        .environment(settings)
        let hostsVC = UIHostingController(rootView: AnyView(screen))
        let nav = UINavigationController(rootViewController: hostsVC)
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
    }

    // MARK: - Nav pushes

    private func pushHostForm(
        id: SavedHost.ID?,
        connectOnSave: Bool,
        onFinish: @escaping (ConnectionFormScreen.Outcome) -> Void
    ) {
        if settings.useRustConnectForm {
            pushRustHostForm(id: id, connectOnSave: connectOnSave, onFinish: onFinish)
            return
        }
        guard let nav = navigationController else { return }
        // Hold onto a closure that the form view captures; we want to
        // pop after the form signals completion regardless of outcome.
        var formVC: UIHostingController<AnyView>?
        let form = ConnectionFormScreen(
            editingID: id,
            connectOnSave: connectOnSave,
            onFinish: { [weak self] outcome in
                onFinish(outcome)
                if let vc = formVC, self?.navigationController?.topViewController === vc {
                    self?.navigationController?.popViewController(animated: true)
                } else {
                    self?.navigationController?.popToRootViewController(animated: true)
                }
            }
        )
        .environment(\.toaster, toaster)
        .environment(settings)
        let host = UIHostingController(rootView: AnyView(form))
        formVC = host
        nav.pushViewController(host, animated: true)
    }

    /// Heap-boxed coordinator captured by the Rust connect-form VC's C
    /// callbacks so they can hop back into Swift on completion.
    private final class FormBox {
        weak var nav: UINavigationController?
        let connectOnSave: Bool
        let onFinish: (ConnectionFormScreen.Outcome) -> Void
        init(
            nav: UINavigationController?,
            connectOnSave: Bool,
            onFinish: @escaping (ConnectionFormScreen.Outcome) -> Void
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
            let outcome: ConnectionFormScreen.Outcome
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

    /// W24c experimental: push the Rust-built connect form. Persistence
    /// still routes through the Swift `ConnectionFormViewModel` via the
    /// `bt_swift_connect_form_*` bridge.
    private func pushRustHostForm(
        id: SavedHost.ID?,
        connectOnSave: Bool,
        onFinish: @escaping (ConnectionFormScreen.Outcome) -> Void
    ) {
        guard let nav = navigationController else { return }
        let box = FormBox(nav: nav, connectOnSave: connectOnSave, onFinish: onFinish)
        let ctx = Unmanaged.passRetained(box).toOpaque()

        // Install the bridge store so the Rust VC's prefill/save calls
        // hit the same persistence path as the SwiftUI form.
        ConnectFormBridge.store = HostsStore()

        let raw: UnsafeMutableRawPointer?
        if let id {
            raw = id.uuidString.withCString { idPtr in
                bt_ios_create_connect_form_vc(idPtr, Self.formBoxOnDone, Self.formBoxOnCancel, ctx)
            }
        } else {
            raw = bt_ios_create_connect_form_vc(nil, Self.formBoxOnDone, Self.formBoxOnCancel, ctx)
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
