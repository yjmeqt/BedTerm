import SwiftUI
import UIKit

/// Root saved-hosts list. Pushed onto the app's `UINavigationController`
/// as the first SwiftUI hosting controller. Navigation pushes (host form,
/// terminal) are dispatched through closures so the `RootCoordinator`
/// owns the actual `UIViewController` push — there's no inner SwiftUI
/// `NavigationStack`.
public struct HostsScreen: View {
    @Environment(\.toaster) var toaster
    @Environment(BedTermSettings.self) var settings
    @State var viewModel = HostsViewModel()
    @State var didFirstAppear = false
    @State var showingMismatchReview = false
    @State var deviceLockedToastID: UUID?
    @State var mismatchToastID: UUID?
    /// Set true while the first-run shortcut form is on-screen, so the form's
    /// primary action becomes "Save & Connect" instead of "Save".
    @State var pendingConnectOnSave = false

    /// Asks the coordinator to push the add/edit form. `nil` id means
    /// "add a new host"; non-nil means "edit existing entry".
    /// `connectOnSave` mirrors the first-run shortcut behaviour. The
    /// coordinator wraps `ConnectionFormScreen` in a `UIHostingController`
    /// and pushes it; when the form finishes it invokes `onFinish` and
    /// pops itself.
    public typealias ShowHostFormHandler = (
        _ id: SavedHost.ID?,
        _ connectOnSave: Bool,
        _ onFinish: @escaping (ConnectionFormScreen.Outcome) -> Void
    ) -> Void

    /// Asks the coordinator to push a pre-built terminal `UIViewController`
    /// onto the nav stack. The screen constructs the VC (it has internal
    /// access to `TerminalSession`) so the app target never needs to see
    /// internal BedTermKit types.
    public typealias ShowTerminalHandler = (UIViewController) -> Void

    /// Asks the coordinator to modally present the settings screen.
    /// The coordinator builds the wrapping `UINavigationController` around
    /// the Rust-backed `BtIosSettingsViewController`.
    public typealias ShowSettingsHandler = () -> Void

    let onShowHostForm: ShowHostFormHandler
    let onShowTerminal: ShowTerminalHandler
    /// Pops the topmost pushed VC (used after a kill).
    let onPopToHosts: () -> Void
    let onShowSettings: ShowSettingsHandler

    private static let firstRunShortcutKey = "hosts.firstRunShortcutDone"

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
        ZStack { rootContent }
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

    @ViewBuilder
    private var rootContent: some View {
        if viewModel.entries.isEmpty && !viewModel.loadFailed {
            emptyState
        } else {
            hostsList
        }
    }

    private var hostsList: some View {
        ScrollView {
            LazyVStack(spacing: 12) {
                ForEach(viewModel.entries) { entry in
                    HostRow(
                        entry: entry,
                        inFlight: viewModel.inFlightID == entry.id,
                        isCurrentSession: viewModel.currentSessionID == entry.id,
                        // Tapping the row body when a session is live
                        // re-enters the running terminal; otherwise it's
                        // a no-op (Connect button handles fresh starts).
                        onTapBody: {
                            if viewModel.currentSessionID == entry.id {
                                pushTerminal()
                            }
                        },
                        onConnect: {
                            viewModel.onConnectError = handleConnectError
                            viewModel.requestConnect(id: entry.id)
                        }
                    )
                    .contextMenu {
                        Button(String(localized: "Edit"), systemImage: "pencil") {
                            presentHostForm(id: entry.id)
                        }
                        Button(String(localized: "Delete"), systemImage: "trash", role: .destructive) {
                            viewModel.requestDelete(id: entry.id)
                        }
                    }
                }
            }
            .padding(16)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 16) {
            Image(systemName: "server.rack")
                .font(.title)
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                .frame(width: 56, height: 56)
                .background(
                    RoundedRectangle(cornerRadius: 12)
                        .fill(Color("ShadcnCard", bundle: .module))
                        .overlay(
                            RoundedRectangle(cornerRadius: 12)
                                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
                        )
                )
            VStack(spacing: 4) {
                Text("No hosts yet")
                    .font(.headline)
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                Text("Add a server to connect from your bed.")
                    .font(.subheadline)
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            }
            Button {
                presentHostForm(id: nil)
            } label: {
                Text("Add Host")
                    .font(.footnote.weight(.medium))
                    .padding(.horizontal, 16)
                    .frame(height: 36)
                    .foregroundStyle(Color("ShadcnPrimaryForeground", bundle: .module))
                    .background(Color("ShadcnPrimary", bundle: .module))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("hosts.emptyState.add")
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // Push + form-outcome helpers live in HostsScreen+Push.swift.

    // MARK: - Toast wiring

    func handleConnectError(id: UUID, message: String, permissionDenied: Bool) {
        let name = viewModel.displayName(for: id)
        var actions: [Toaster.Action] = []
        if permissionDenied {
            actions.append(
                Toaster.Action(String(localized: "Open Settings")) { openSettings() }
            )
        }
        actions.append(
            Toaster.Action(String(localized: "Retry")) {
                viewModel.onConnectError = handleConnectError
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
                        Toaster.Action(String(localized: "Retry")) {
                            viewModel.retryLoad()
                        }
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
                Toaster.Action(String(localized: "Review")) {
                    showingMismatchReview = true
                }
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

    // MARK: - Lifecycle

    private func onAppear() {
        viewModel.load()
        // Settings env is unavailable at view-init time; wire the
        // bootstrap-payload resolver here so the saved-host Connect
        // path can push the shell-integration heredoc when the user
        // has the toggle on. `[settings]` capture is Sendable because
        // BedTermSettings is `@MainActor @Observable` and the closure
        // runs on the main actor.
        viewModel.bootstrapPayloadProvider = { [settings] in
            guard settings.showCommandBlocks else { return nil }
            return ShellIntegrationScript.bootstrapPayload()
        }
        guard !didFirstAppear else { return }
        didFirstAppear = true
        let defaults = UserDefaults.standard
        let alreadyShortcut = defaults.bool(forKey: Self.firstRunShortcutKey)
        if !alreadyShortcut && viewModel.entries.isEmpty {
            defaults.set(true, forKey: Self.firstRunShortcutKey)
            pendingConnectOnSave = true
            presentHostForm(id: nil)
        }
    }

    private func openSettings() {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
    }
}
