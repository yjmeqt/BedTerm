import BedTermCoreC
import SwiftUI
import UIKit

/// W24b phase 1 wrapper around the Rust-built
/// `BtIosHostsListViewController`. Owns the same `HostsViewModel` the
/// SwiftUI `HostsScreen` does, wires the toaster + terminal-push
/// callbacks identically, and lets the Rust VC handle row layout +
/// swipe + `+` button. Mismatch / swap / delete dialogs are reused from
/// `HostsScreen`'s modifier stack so we don't reimplement the
/// confirmation chrome.
public struct RustHostsScreen: View {
    @Environment(\.toaster) var toaster
    @Environment(BedTermSettings.self) var settings
    @State private var viewModel = HostsViewModel()
    @State private var didFirstAppear = false
    @State private var showingMismatchReview = false
    @State private var mismatchToastID: UUID?
    @State private var deviceLockedToastID: UUID?
    /// Bumped on every `onAppear` so SwiftUI re-invokes
    /// `updateUIViewController`, which in turn drives the Rust VC's
    /// `viewWillAppear` snapshot refresh. Without this, returning from a
    /// pushed terminal VC doesn't trigger a body diff (entry count is
    /// unchanged) and the row list goes stale.
    @State private var refreshNonce = 0

    public typealias ShowHostFormHandler = HostsScreen.ShowHostFormHandler
    public typealias ShowTerminalHandler = HostsScreen.ShowTerminalHandler
    public typealias ShowSettingsHandler = HostsScreen.ShowSettingsHandler

    private let onShowHostForm: ShowHostFormHandler
    private let onShowTerminal: ShowTerminalHandler
    private let onPopToHosts: () -> Void
    private let onShowSettings: ShowSettingsHandler

    public init(
        onShowHostForm: @escaping ShowHostFormHandler,
        onShowTerminal: @escaping ShowTerminalHandler,
        onPopToHosts: @escaping () -> Void,
        onShowSettings: @escaping ShowSettingsHandler
    ) {
        self.onShowHostForm = onShowHostForm
        self.onShowTerminal = onShowTerminal
        self.onPopToHosts = onPopToHosts
        self.onShowSettings = onShowSettings
    }

    public var body: some View {
        RustHostsVCRepresentable(
            onAdd: { presentHostForm(id: nil) },
            // Both the entry count (form save / delete) and the
            // appear-nonce (back-from-terminal) feed into the diff so
            // the Rust VC always re-pulls the snapshot when this view
            // becomes visible.
            refreshTrigger: viewModel.entries.count &+ refreshNonce
        )
        .ignoresSafeArea()
        .background(Color("ShadcnBackground", bundle: .module).ignoresSafeArea())
        .navigationTitle(Text("Hosts"))
        .toolbar { toolbarContent }
        .modifier(SwapDialogModifier(viewModel: viewModel))
        .modifier(DeleteDialogModifier(viewModel: viewModel))
        .sheet(isPresented: $showingMismatchReview) {
            if let mismatch = viewModel.pendingMismatch {
                HostKeyMismatchReviewSheet(
                    mismatch: mismatch,
                    onTrust: {
                        showingMismatchReview = false
                        dismissMismatchToast()
                        viewModel.retryAfterMismatch()
                    },
                    onReject: {
                        showingMismatchReview = false
                        dismissMismatchToast()
                        viewModel.clearMismatch()
                    }
                )
            }
        }
        .onAppear(perform: onAppear)
        .onChange(of: viewModel.loadFailed) { _, locked in
            handleLoadFailed(locked)
        }
        .onChange(of: viewModel.currentSessionID) { _, new in
            if new != nil { pushTerminal() }
        }
        .onChange(of: viewModel.pendingMismatch?.sourceID) { _, _ in
            handlePendingMismatchChanged()
        }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button {
                onShowSettings()
            } label: {
                Image(systemName: "gearshape")
            }
            .accessibilityLabel(Text("Settings"))
            .accessibilityIdentifier("hosts.settings")
        }
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                presentHostForm(id: nil)
            } label: {
                Image(systemName: "plus")
            }
            .accessibilityLabel(Text("Add Host"))
            .accessibilityIdentifier("hosts.add")
        }
    }

    // MARK: - Lifecycle

    private func onAppear() {
        // Re-point the bridge at the screen's view-model so row taps
        // hit this connect flow. The bridge is a global enum — multiple
        // screens would race, but at any given time only one screen of
        // either flavour is rooted in the nav.
        HostsBridge.store = HostsStore()
        HostsBridge.connectHandler = { id in
            self.viewModel.requestConnect(id: id)
        }

        // Bump the refresh nonce so `updateUIViewController` runs and
        // drives the Rust VC's snapshot reload. This is what makes the
        // hosts list re-populate when the user navigates back from a
        // pushed terminal screen.
        refreshNonce &+= 1

        viewModel.load()
        viewModel.bootstrapPayloadProvider = { [settings] in
            guard settings.showCommandBlocks else { return nil }
            return ShellIntegrationScript.bootstrapPayload()
        }
        viewModel.onConnectError = { id, message, perm in
            self.handleConnectError(id: id, message: message, permissionDenied: perm)
        }
        guard !didFirstAppear else { return }
        didFirstAppear = true
    }

    private func presentHostForm(id: SavedHost.ID?) {
        onShowHostForm(id, false) { outcome in
            viewModel.refresh()
            switch outcome {
            case .savedAndConnect(let newID):
                toaster.show(
                    .success,
                    title: String(localized: "Host saved"),
                    description: String(localized: "Connecting to \(viewModel.displayName(for: newID))…")
                )
                viewModel.connect(id: newID)
            case .saved(let newID):
                toaster.show(
                    .success,
                    title: String(localized: "Host saved"),
                    description: viewModel.displayName(for: newID)
                )
            case .cancelled:
                break
            }
        }
    }

    private func pushTerminal() {
        guard let session = viewModel.lastSession,
            let id = viewModel.currentSessionID,
            let entry = viewModel.entries.first(where: { $0.id == id })
        else { return }
        let hostName = viewModel.displayName(for: id)
        let entryID = entry.id
        let payloadProvider: @MainActor () -> String? = { [settings] in
            guard settings.showCommandBlocks else { return nil }
            return ShellIntegrationScript.bootstrapPayload()
        }
        let pop = onPopToHosts
        let onKill: () -> Void = {
            let label = self.viewModel.displayName(for: entryID)
            self.viewModel.endLiveSession()
            self.toaster.show(
                .info,
                title: String(localized: "Session ended"),
                description: String(localized: "Disconnected from \(label).")
            )
            pop()
        }
        let vc = TerminalScreenViewController(
            session: session,
            credential: entry.credential,
            hostName: hostName,
            bootstrapPayloadProvider: payloadProvider,
            onBack: pop,
            onKill: onKill
        )
        onShowTerminal(vc)
    }

    private func handleConnectError(id: UUID, message: String, permissionDenied: Bool) {
        let name = viewModel.displayName(for: id)
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
            Toaster.Action(String(localized: "Retry")) {
                viewModel.requestConnect(id: id)
            }
        )
        toaster.show(
            .error,
            title: String(localized: "Connection failed · \(name)"),
            description: message,
            actions: actions
        )
    }

    private func handleLoadFailed(_ locked: Bool) {
        if locked {
            if deviceLockedToastID == nil {
                deviceLockedToastID = toaster.show(
                    .warning,
                    title: String(localized: "Saved hosts unavailable"),
                    description: String(
                        localized: "Unlock your device to access stored credentials."),
                    actions: [
                        Toaster.Action(String(localized: "Retry")) { viewModel.retryLoad() }
                    ],
                    persistent: true
                )
            }
        } else if let id = deviceLockedToastID {
            toaster.dismiss(id: id)
            deviceLockedToastID = nil
        }
    }

    private func handlePendingMismatchChanged() {
        if let id = mismatchToastID {
            toaster.dismiss(id: id)
            mismatchToastID = nil
        }
        guard let mismatch = viewModel.pendingMismatch else { return }
        mismatchToastID = toaster.show(
            .warning,
            title: String(localized: "Host key changed · \(mismatch.host)"),
            description: String(localized: "Tap to review and accept or reject."),
            actions: [
                Toaster.Action(String(localized: "Review")) { showingMismatchReview = true }
            ],
            persistent: true
        )
    }

    private func dismissMismatchToast() {
        if let id = mismatchToastID {
            toaster.dismiss(id: id)
            mismatchToastID = nil
        }
    }
}

// MARK: - UIViewControllerRepresentable

/// SwiftUI wrapper around `BtIosHostsListViewController`. `refreshTrigger`
/// is keyed off the view-model entry count so SwiftUI re-invokes
/// `updateUIViewController` whenever the list mutates, giving the Rust VC
/// a chance to pull a fresh snapshot through the bridge.
private struct RustHostsVCRepresentable: UIViewControllerRepresentable {
    let onAdd: () -> Void
    let refreshTrigger: Int

    func makeCoordinator() -> Coordinator {
        Coordinator(onAdd: onAdd)
    }

    func makeUIViewController(context: Context) -> UIViewController {
        // The C `+` callback needs an opaque context. Heap-box the
        // coordinator so the C function can find back to the SwiftUI
        // closure. Strongly retained by the wrapping VC (associated
        // object) so the box lives as long as the Rust VC.
        let box = context.coordinator
        let ctx = Unmanaged.passRetained(box).toOpaque()
        let callback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
            guard let ctx else { return }
            let box = Unmanaged<Coordinator>.fromOpaque(ctx).takeUnretainedValue()
            DispatchQueue.main.async {
                box.onAdd()
            }
        }
        guard let raw = bt_ios_create_hosts_list_vc(callback, ctx) else {
            Unmanaged<Coordinator>.fromOpaque(ctx).release()
            return UIViewController()
        }
        let vc = Unmanaged<UIViewController>.fromOpaque(raw).takeRetainedValue()
        // Pin the coordinator to the VC lifetime so the +1 retain we
        // took is balanced when the VC is dropped.
        objc_setAssociatedObject(
            vc, &Self.coordinatorKey, box, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        Unmanaged<Coordinator>.fromOpaque(ctx).release()
        return vc
    }

    func updateUIViewController(_ uiViewController: UIViewController, context: Context) {
        context.coordinator.onAdd = onAdd
        // Drive a snapshot refresh — the Rust VC re-pulls `HostsBridge`
        // JSON on `viewWillAppear`, so flip the appearance transition.
        uiViewController.beginAppearanceTransition(true, animated: false)
        uiViewController.endAppearanceTransition()
    }

    private static var coordinatorKey: UInt8 = 0

    @MainActor
    final class Coordinator {
        var onAdd: () -> Void
        init(onAdd: @escaping () -> Void) {
            self.onAdd = onAdd
        }
    }
}
