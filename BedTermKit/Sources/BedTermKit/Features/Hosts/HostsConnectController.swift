import BedTermIOS
import Observation
import SwiftUI
import UIKit

/// Owns the Rust `BtIosHostsListViewController` and the orchestration
/// around it: connect kick-off, swap / delete alerts, mismatch review
/// sheet, toaster messaging, terminal-push, first-run shortcut. W24d
/// retired the SwiftUI `HostsScreen` / `RustHostsScreen`; this
/// UIKit-native controller observes `HostsViewModel` instead.
@MainActor
public final class HostsConnectController {
    public typealias ShowHostForm = (
        SavedHost.ID?, Bool, @escaping (ConnectionFormOutcome) -> Void
    ) -> Void
    public typealias ShowTerminal = (UIViewController) -> Void
    public typealias ShowSettings = () -> Void

    /// `BtIosHostsListViewController` wrapped in a UIKit container so
    /// we can carry the nav item / toolbar buttons. `RootCoordinator`
    /// embeds this into the root `UINavigationController`.
    public let rootViewController: UIViewController

    // Internal: dialog extension reads swap/delete state.
    let viewModel = HostsViewModel()
    private let toaster: Toaster
    private let onShowHostForm: ShowHostForm
    private let onShowTerminal: ShowTerminal
    private let onShowSettings: ShowSettings
    private let onPopToHosts: () -> Void

    /// Set true while the first-run shortcut form is on-screen.
    private var pendingConnectOnSave = false
    private var didFirstAppear = false

    private var deviceLockedToastID: UUID?
    private var mismatchToastID: UUID?
    private weak var presentedMismatchHost: UIViewController?
    // Internal: dismissed by the dialog extension on VM clear.
    weak var presentedSwapAlert: UIAlertController?
    weak var presentedDeleteAlert: UIAlertController?

    private static let firstRunShortcutKey = "hosts.firstRunShortcutDone"

    /// Heap box so the C `+` callback can call back into the
    /// controller without `Unmanaged` retain juggling on each tap.
    private final class AddBox {
        weak var owner: HostsConnectController?
        init(owner: HostsConnectController?) { self.owner = owner }
    }
    private let addBox: AddBox
    private static var addBoxKey: UInt8 = 0
    private static var rustChildKey: UInt8 = 0

    private var observerToken: Int = 0

    public init(
        toaster: Toaster,
        onShowHostForm: @escaping ShowHostForm,
        onShowTerminal: @escaping ShowTerminal,
        onPopToHosts: @escaping () -> Void,
        onShowSettings: @escaping ShowSettings
    ) {
        self.toaster = toaster
        self.onShowHostForm = onShowHostForm
        self.onShowTerminal = onShowTerminal
        self.onPopToHosts = onPopToHosts
        self.onShowSettings = onShowSettings
        self.addBox = AddBox(owner: nil)

        // Container VC carries the nav-bar items; the Rust VC lives
        // inside as a child. Rows / swipe / connect go through
        // `HostsBridge`; the `+` button is the only explicit callback.
        let container = UIViewController()
        container.view.backgroundColor = UIColor(
            named: "ShadcnBackground", in: .module, compatibleWith: nil)
        self.rootViewController = container
        self.addBox.owner = self

        // Wire the bridge connect handler + load entries before the
        // Rust VC's viewDidLoad pulls a snapshot — otherwise first
        // render flashes empty. The snapshot itself is now read
        // directly from `crate::hosts_store` by the Rust VC.
        HostsBridge.connectHandler = { [weak self] id in
            self?.viewModel.requestConnect(id: id)
        }
        self.viewModel.load()

        let ctx = Unmanaged.passUnretained(self.addBox).toOpaque()
        let callback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
            guard let ctx else { return }
            let box = Unmanaged<AddBox>.fromOpaque(ctx).takeUnretainedValue()
            DispatchQueue.main.async {
                box.owner?.presentHostForm(id: nil)
            }
        }
        if let raw = bt_ios_create_hosts_list_vc(callback, ctx) {
            let vc = Unmanaged<UIViewController>.fromOpaque(raw).takeRetainedValue()
            objc_setAssociatedObject(
                container, &Self.addBoxKey, self.addBox, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
            objc_setAssociatedObject(
                container, &Self.rustChildKey, vc, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
            self.embedRustChild(vc, in: container)
        }

        self.configureNavigationItem()
        self.wireBridge()
        self.armObservers()
    }

    // MARK: - Embed

    private func embedRustChild(_ child: UIViewController, in target: UIViewController) {
        target.addChild(child)
        child.view.translatesAutoresizingMaskIntoConstraints = false
        target.view.addSubview(child.view)
        NSLayoutConstraint.activate([
            child.view.topAnchor.constraint(equalTo: target.view.topAnchor),
            child.view.bottomAnchor.constraint(equalTo: target.view.bottomAnchor),
            child.view.leadingAnchor.constraint(equalTo: target.view.leadingAnchor),
            child.view.trailingAnchor.constraint(equalTo: target.view.trailingAnchor)
        ])
        child.didMove(toParent: target)
    }

    private func configureNavigationItem() {
        let nav = self.rootViewController.navigationItem
        nav.title = String(localized: "Hosts")

        let settingsButton = UIBarButtonItem(
            image: UIImage(systemName: "gearshape"),
            style: .plain,
            target: self,
            action: #selector(handleSettingsTap)
        )
        settingsButton.accessibilityIdentifier = "hosts.settings"
        settingsButton.accessibilityLabel = String(localized: "Settings")
        nav.leftBarButtonItem = settingsButton

        let addButton = UIBarButtonItem(
            image: UIImage(systemName: "plus"),
            style: .plain,
            target: self,
            action: #selector(handleAddTap)
        )
        addButton.accessibilityIdentifier = "hosts.add"
        addButton.accessibilityLabel = String(localized: "Add Host")
        nav.rightBarButtonItem = addButton
    }

    @objc private func handleSettingsTap() { self.onShowSettings() }
    @objc private func handleAddTap() { self.presentHostForm(id: nil) }

    private func refreshRustSnapshot() {
        guard
            let rust = objc_getAssociatedObject(self.rootViewController, &Self.rustChildKey)
                as? UIViewController
        else { return }
        // The Rust VC re-pulls from `hosts_store` in its
        // viewWillAppear; flip the appearance transition to force it.
        rust.beginAppearanceTransition(true, animated: false)
        rust.endAppearanceTransition()
    }

    private func wireBridge() {
        self.viewModel.bootstrapPayloadProvider = {
            guard bt_ios_settings_show_command_blocks() else { return nil }
            return ShellIntegrationScript.bootstrapPayload()
        }
        self.viewModel.onConnectError = { [weak self] id, message, perm in
            self?.handleConnectError(id: id, message: message, permissionDenied: perm)
        }
    }

    // MARK: - Lifecycle entry

    /// Called by `RootCoordinator` after install. Re-loads (init
    /// loaded too, but the store may have mutated since) and runs the
    /// first-run shortcut if appropriate.
    public func start() {
        self.viewModel.load()
        guard !self.didFirstAppear else { return }
        self.didFirstAppear = true
        let defaults = UserDefaults.standard
        let alreadyShortcut = defaults.bool(forKey: Self.firstRunShortcutKey)
        if !alreadyShortcut && self.viewModel.entries.isEmpty {
            defaults.set(true, forKey: Self.firstRunShortcutKey)
            self.pendingConnectOnSave = true
            self.presentHostForm(id: nil)
        }
        self.refreshRustSnapshot()
    }

    // MARK: - Observation

    /// Re-arm one-shot `withObservationTracking` on each VM change.
    private func armObservers() {
        let token = self.observerToken + 1
        self.observerToken = token
        withObservationTracking {
            _ = self.viewModel.entries.count
            _ = self.viewModel.loadFailed
            _ = self.viewModel.currentSessionID
            _ = self.viewModel.pendingMismatch?.sourceID
            _ = self.viewModel.swapConfirmation?.targetID
            _ = self.viewModel.deleteConfirmation?.targetID
        } onChange: { [weak self] in
            DispatchQueue.main.async {
                guard let self else { return }
                guard token == self.observerToken else { return }
                self.handleObservedChange()
                self.armObservers()
            }
        }
    }

    private func handleObservedChange() {
        self.refreshRustSnapshot()
        self.handleLoadFailed(self.viewModel.loadFailed)
        if self.viewModel.currentSessionID != nil { self.pushTerminal() }
        self.handlePendingMismatchChanged()
        self.handleSwapConfirmationChanged()
        self.handleDeleteConfirmationChanged()
    }

    // MARK: - Form push

    private func presentHostForm(id: SavedHost.ID?) {
        let connectOnSave = self.pendingConnectOnSave && id == nil
        self.onShowHostForm(id, connectOnSave) { [weak self] outcome in
            self?.handleFormOutcome(outcome)
        }
    }

    private func handleFormOutcome(_ outcome: ConnectionFormOutcome) {
        self.viewModel.refresh()
        self.pendingConnectOnSave = false
        switch outcome {
        case .savedAndConnect(let id):
            self.toaster.show(
                .success,
                title: String(localized: "Host saved"),
                description: String(
                    localized: "Connecting to \(self.viewModel.displayName(for: id))…")
            )
            self.viewModel.connect(id: id)
        case .saved(let id):
            self.toaster.show(
                .success,
                title: String(localized: "Host saved"),
                description: self.viewModel.displayName(for: id)
            )
        case .cancelled:
            break
        }
    }

    // MARK: - Terminal push

    private func pushTerminal() {
        guard let session = self.viewModel.lastSession,
            let id = self.viewModel.currentSessionID,
            let entry = self.viewModel.entries.first(where: { $0.id == id })
        else { return }
        let hostName = self.viewModel.displayName(for: id)
        let entryID = entry.id
        let payloadProvider: @MainActor () -> String? = {
            guard bt_ios_settings_show_command_blocks() else { return nil }
            return ShellIntegrationScript.bootstrapPayload()
        }
        let onBack: () -> Void = { [weak self] in
            guard let self else { return }
            let label = self.viewModel.displayName(for: entryID)
            self.viewModel.endLiveSession()
            self.toaster.show(
                .info,
                title: String(localized: "Session ended"),
                description: String(localized: "Disconnected from \(label).")
            )
            self.onPopToHosts()
        }
        let vc = TerminalScreenViewController(
            session: session,
            credential: entry.credential,
            hostName: hostName,
            bootstrapPayloadProvider: payloadProvider,
            onBack: onBack
        )
        self.onShowTerminal(vc)
    }

    // MARK: - Toast + alert chrome

    private func handleConnectError(id: UUID, message: String, permissionDenied: Bool) {
        let name = self.viewModel.displayName(for: id)
        var actions: [Toaster.Action] = []
        if permissionDenied {
            actions.append(
                Toaster.Action(String(localized: "Open Settings")) {
                    if let url = URL(string: UIApplication.openSettingsURLString) {
                        UIApplication.shared.open(url)
                    }
                }
            )
        }
        actions.append(
            Toaster.Action(String(localized: "Retry")) { [weak self] in
                self?.viewModel.requestConnect(id: id)
            }
        )
        self.toaster.show(
            .error,
            title: String(localized: "Connection failed · \(name)"),
            description: message,
            actions: actions
        )
    }

    private func handleLoadFailed(_ locked: Bool) {
        if locked {
            if self.deviceLockedToastID == nil {
                self.deviceLockedToastID = self.toaster.show(
                    .warning,
                    title: String(localized: "Saved hosts unavailable"),
                    description: String(
                        localized: "Unlock your device to access stored credentials."),
                    actions: [
                        Toaster.Action(String(localized: "Retry")) { [weak self] in
                            self?.viewModel.retryLoad()
                        }
                    ],
                    persistent: true
                )
            }
        } else if let id = self.deviceLockedToastID {
            self.toaster.dismiss(id: id)
            self.deviceLockedToastID = nil
        }
    }

    private func handlePendingMismatchChanged() {
        if let id = self.mismatchToastID {
            self.toaster.dismiss(id: id)
            self.mismatchToastID = nil
        }
        // Title / body / action label all come from Rust — see
        // `hosts_vm::mismatch_alert`. Swift owns toast presentation only.
        guard let text = Self.readAlertText(bt_ios_hosts_vm_mismatch_alert()) else { return }
        self.mismatchToastID = self.toaster.show(
            .warning,
            title: text.title,
            description: text.message,
            actions: [
                Toaster.Action(text.confirmLabel) { [weak self] in
                    self?.presentMismatchReview()
                }
            ],
            persistent: true
        )
    }

    private func presentMismatchReview() {
        guard let mismatch = self.viewModel.pendingMismatch else { return }
        let sheet = HostKeyMismatchReviewSheet(
            mismatch: mismatch,
            onTrust: { [weak self] in
                self?.dismissMismatchReview()
                self?.dismissMismatchToast()
                self?.viewModel.retryAfterMismatch()
            },
            onReject: { [weak self] in
                self?.dismissMismatchReview()
                self?.dismissMismatchToast()
                self?.viewModel.clearMismatch()
            }
        )
        let host = UIHostingController(rootView: sheet)
        host.modalPresentationStyle = .pageSheet
        self.presentedMismatchHost = host
        self.rootViewController.present(host, animated: true)
    }

    private func dismissMismatchReview() {
        self.presentedMismatchHost?.dismiss(animated: true)
        self.presentedMismatchHost = nil
    }

    private func dismissMismatchToast() {
        if let id = self.mismatchToastID {
            self.toaster.dismiss(id: id)
            self.mismatchToastID = nil
        }
    }

    // Swap/delete alerts: `HostsConnectController+Dialogs.swift`.
}
