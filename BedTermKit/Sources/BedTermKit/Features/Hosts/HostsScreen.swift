import SwiftUI
import UIKit

public struct HostsScreen: View {
    @Binding var path: NavigationPath
    @State private var viewModel = HostsViewModel()
    @State private var didFirstAppear = false
    /// Set true while the first-run shortcut form is on-screen, so the form's
    /// primary action becomes "Save & Connect" instead of "Save".
    @State private var pendingConnectOnSave = false

    private static let firstRunShortcutKey = "hosts.firstRunShortcutDone"

    public init(path: Binding<NavigationPath>) {
        self._path = path
    }

    public var body: some View {
        decoratedRoot
    }

    private var decoratedRoot: some View {
        rootContent
            .navigationTitle(Text("Hosts"))
            .toolbar { toolbarContent }
            .navigationDestination(for: AppRoute.self) { route in
                destination(for: route)
            }
            .modifier(SwapDialogModifier(viewModel: viewModel))
            .modifier(DeleteDialogModifier(viewModel: viewModel))
            .onAppear(perform: onAppear)
            .onChange(of: viewModel.currentSessionID) { _, new in
                if new != nil { path.append(AppRoute.terminal) }
            }
            .onChange(of: viewModel.pendingMismatch?.sourceID) { _, _ in
                pushMismatchIfNeeded()
            }
            .privacySensitive()
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
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

    private func pushMismatchIfNeeded() {
        guard let mismatch = viewModel.pendingMismatch else { return }
        path.append(
            AppRoute.hostKeyMismatch(
                stored: mismatch.stored,
                remote: mismatch.remote,
                host: mismatch.host,
                port: mismatch.port
            ))
    }

    @ViewBuilder
    private var rootContent: some View {
        if viewModel.loadFailed {
            lockedBanner
        } else if viewModel.entries.isEmpty {
            emptyState
        } else {
            hostsList
        }
    }

    private var hostsList: some View {
        List {
            ForEach(viewModel.entries) { entry in
                HostRow(
                    entry: entry,
                    inFlight: viewModel.inFlightID == entry.id,
                    error: viewModel.errorByID[entry.id],
                    isCurrentSession: viewModel.currentSessionID == entry.id,
                    onRowBodyTap: { viewModel.onRowBodyTap(id: entry.id) },
                    onConnect: { viewModel.requestConnect(id: entry.id) },
                    onEdit: { path.append(AppRoute.hostForm(entry.id)) },
                    onRetry: { viewModel.requestConnect(id: entry.id) },
                    onOpenSettings: openSettings
                )
                .contentShape(Rectangle())
                .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                    Button(role: .destructive) {
                        viewModel.requestDelete(id: entry.id)
                    } label: {
                        Label(String(localized: "Delete"), systemImage: "trash")
                    }
                    Button {
                        path.append(AppRoute.hostForm(entry.id))
                    } label: {
                        Label(String(localized: "Edit"), systemImage: "pencil")
                    }
                    .tint(.blue)
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
        .listStyle(.insetGrouped)
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label(String(localized: "No hosts yet"), systemImage: "server.rack")
        } description: {
            Text("Add a server to connect from your bed.")
        } actions: {
            Button(String(localized: "Add Host")) {
                path.append(AppRoute.hostForm(nil))
            }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("hosts.emptyState.add")
        }
    }

    private var lockedBanner: some View {
        VStack {
            HStack(spacing: 10) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text("Couldn't load saved hosts — device locked.")
                    .font(.callout)
                Spacer()
                Button(String(localized: "Retry")) { viewModel.retryLoad() }
            }
            .padding()
            .background(Color.orange.opacity(0.1))
            Spacer()
        }
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
            if let session = viewModel.lastSession, let entry = currentSessionEntry() {
                TerminalScreen(
                    session: session,
                    credential: entry.credential,
                    onExit: {
                        viewModel.sessionEnded()
                        path = NavigationPath()
                    }
                )
            } else {
                Text("No session.")
            }
        case .hostKeyMismatch(let stored, let remote, let host, let port):
            HostKeyMismatchScreen(
                stored: stored, remote: remote, host: host, port: port,
                onTrust: { viewModel.retryAfterMismatch() },
                onReject: {
                    viewModel.clearMismatch()
                    path = NavigationPath()
                }
            )
        }
    }

    private func currentSessionEntry() -> SavedHost? {
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
            viewModel.connect(id: id)
        case .saved, .cancelled:
            _ = formWasShortcut
        }
    }

    // MARK: - Lifecycle

    private func onAppear() {
        viewModel.load()
        guard !didFirstAppear else { return }
        didFirstAppear = true
        // R1.first_run_form_shortcut: after onboarding, if no saved hosts and we
        // haven't run this shortcut yet, jump straight to the New Host form.
        let defaults = UserDefaults.standard
        let alreadyShortcut = defaults.bool(forKey: Self.firstRunShortcutKey)
        if !alreadyShortcut && viewModel.entries.isEmpty {
            defaults.set(true, forKey: Self.firstRunShortcutKey)
            pendingConnectOnSave = true
            path.append(AppRoute.hostForm(nil))
        }
    }

    private func openSettings() {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
    }
}

// MARK: - Confirmation dialog modifiers

private struct SwapDialogModifier: ViewModifier {
    @Bindable var viewModel: HostsViewModel

    func body(content: Content) -> some View {
        content.confirmationDialog(
            swapTitle,
            isPresented: binding,
            titleVisibility: .visible,
            presenting: viewModel.swapConfirmation
        ) { _ in
            Button(String(localized: "Connect")) { viewModel.confirmSwap() }
            Button(String(localized: "Cancel"), role: .cancel) { viewModel.cancelSwap() }
        } message: { _ in
            Text("Your current SSH session will be disconnected.")
        }
    }

    private var binding: Binding<Bool> {
        Binding(
            get: { viewModel.swapConfirmation != nil },
            set: { if !$0 { viewModel.cancelSwap() } }
        )
    }

    private var swapTitle: String {
        guard let target = viewModel.swapConfirmation else { return "" }
        return String(localized: "End current session and connect to \"\(target.displayName)\"?")
    }
}

private struct DeleteDialogModifier: ViewModifier {
    @Bindable var viewModel: HostsViewModel

    func body(content: Content) -> some View {
        content.confirmationDialog(
            deleteTitle,
            isPresented: binding,
            titleVisibility: .visible,
            presenting: viewModel.deleteConfirmation
        ) { _ in
            Button(String(localized: "Delete"), role: .destructive) { viewModel.confirmDelete() }
            Button(String(localized: "Cancel"), role: .cancel) { viewModel.cancelDelete() }
        } message: { conf in
            if conf.isLive {
                Text("You are currently connected. The session will end and the saved password or key will be removed.")
            } else {
                Text("This will remove the saved password or key.")
            }
        }
    }

    private var binding: Binding<Bool> {
        Binding(
            get: { viewModel.deleteConfirmation != nil },
            set: { if !$0 { viewModel.cancelDelete() } }
        )
    }

    private var deleteTitle: String {
        guard let target = viewModel.deleteConfirmation else { return "" }
        if target.isLive {
            return String(localized: "Disconnect and delete \"\(target.displayName)\"?")
        }
        return String(localized: "Delete \"\(target.displayName)\"?")
    }
}
