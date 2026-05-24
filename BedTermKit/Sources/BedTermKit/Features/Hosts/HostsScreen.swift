import SwiftUI
import UIKit

public struct HostsScreen: View {
    @Binding var path: NavigationPath
    @Environment(\.toaster) var toaster
    @Environment(BedTermSettings.self) private var settings
    @Environment(PersistedSessionSnapshotStore.self) var snapshotStore
    @Environment(\.persistenceHandle) private var persistenceHandle
    @State var viewModel = HostsViewModel()
    @State private var didFirstAppear = false
    @State private var showingMismatchReview = false
    @State private var showingSettings = false
    @State private var deviceLockedToastID: UUID?
    @State private var mismatchToastID: UUID?
    /// Set true while the first-run shortcut form is on-screen, so the form's
    /// primary action becomes "Save & Connect" instead of "Save".
    @State private var pendingConnectOnSave = false

    private static let firstRunShortcutKey = "hosts.firstRunShortcutDone"

    public init(path: Binding<NavigationPath>) {
        self._path = path
    }

    public var body: some View {
        ZStack { rootContent }
            .navigationDestination(for: AppRoute.self) { route in
                destination(for: route)
            }
            .background(Color("ShadcnBackground", bundle: .module).ignoresSafeArea())
            .navigationTitle(Text("Hosts"))
            .toolbar { toolbarContent }
            .modifier(SwapDialogModifier(viewModel: viewModel))
            .modifier(DeleteDialogModifier(viewModel: viewModel))
            .sheet(isPresented: $showingSettings) {
                SettingsScreen()
            }
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
                if new != nil { path.append(AppRoute.terminal) }
            }
            .onChange(of: viewModel.pendingMismatch?.sourceID) { _, _ in
                handlePendingMismatchChanged()
            }
            .onChange(of: persistenceHandle == nil) { _, _ in
                // SQLite opens off-main in BedTermApp.task; rewire once the
                // handle goes non-nil so connect flows actually persist.
                viewModel.persistenceHandle = persistenceHandle
                reloadAllSnapshots()
            }
            .task(id: viewModel.entries.map(\.id)) {
                reloadAllSnapshots()
            }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button {
                showingSettings = true
            } label: {
                Image(systemName: "gearshape")
            }
            .accessibilityLabel(Text("Settings"))
            .accessibilityIdentifier("hosts.settings")
        }
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                path.append(AppRoute.hostForm(nil))
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
                    VStack(alignment: .leading, spacing: 8) {
                        HostRow(
                            entry: entry,
                            inFlight: viewModel.inFlightID == entry.id,
                            isCurrentSession: viewModel.currentSessionID == entry.id,
                            // Tapping the row body opens the full panel
                            // unconditionally; the inline list below
                            // covers the common low-count case so most
                            // users won't reach for the body tap.
                            onTapBody: { path.append(AppRoute.sessionsPanel(entry.id)) },
                            onConnect: {
                                viewModel.onConnectError = handleConnectError
                                viewModel.requestConnect(id: entry.id)
                            }
                        )
                        HostSessionsInlineList(
                            host: entry,
                            runningSessionID: viewModel.currentSessionID,
                            snapshots: snapshotStore.snapshots(forHost: entry.id),
                            onTapRunning: {
                                path.append(AppRoute.terminal)
                            },
                            onTapKilled: { snapshot in
                                path.append(AppRoute.killedSessionDetail(snapshot.id))
                            },
                            onViewAll: {
                                path.append(AppRoute.sessionsPanel(entry.id))
                            }
                        )
                    }
                    .contextMenu {
                        Button(String(localized: "Edit"), systemImage: "pencil") {
                            path.append(AppRoute.hostForm(entry.id))
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
                path.append(AppRoute.hostForm(nil))
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

    // MARK: - Destination

    @ViewBuilder
    private func destination(for route: AppRoute) -> some View {
        switch route {
        case .hostForm(let id):
            ConnectionFormScreen(
                editingID: id,
                connectOnSave: pendingConnectOnSave && id == nil,
                onFinish: handleFormOutcome
            )
        case .terminal:
            terminalDestination
        case .sessionsPanel(let hostID):
            sessionsPanelDestination(hostID: hostID)
        case .killedSessionDetail(let snapshotID):
            killedSessionDetailDestination(snapshotID: snapshotID)
        }
    }

    // Destination helpers live in HostsScreen+Destinations.swift.

    func currentSessionEntry() -> SavedHost? {
        guard let id = viewModel.currentSessionID else { return nil }
        return viewModel.entries.first { $0.id == id }
    }

    private func handleFormOutcome(_ outcome: ConnectionFormScreen.Outcome) {
        viewModel.refresh()
        if !path.isEmpty { path.removeLast() }
        let formWasShortcut = pendingConnectOnSave
        pendingConnectOnSave = false
        switch outcome {
        case .savedAndConnect(let id):
            toaster.show(
                .success,
                title: String(localized: "Host saved"),
                description: String(localized: "Connecting to \(viewModel.displayName(for: id))…")
            )
            viewModel.onConnectError = handleConnectError
            viewModel.connect(id: id)
        case .saved(let id):
            toaster.show(
                .success,
                title: String(localized: "Host saved"),
                description: viewModel.displayName(for: id)
            )
        case .cancelled:
            _ = formWasShortcut
        }
    }

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
        viewModel.snapshotStore = snapshotStore
        viewModel.persistenceHandle = persistenceHandle
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
            path.append(AppRoute.hostForm(nil))
        }
    }

    private func reloadAllSnapshots() {
        guard persistenceHandle != nil else { return }
        for entry in viewModel.entries {
            snapshotStore.reload(forHost: entry.id)
        }
    }

    private func openSettings() {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
    }
}
