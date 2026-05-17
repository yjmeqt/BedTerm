import SwiftUI
import UIKit

public struct HostsScreen: View {
    @Binding var path: NavigationPath
    @Environment(\.toaster) private var toaster
    @State private var viewModel = HostsViewModel()
    @State private var didFirstAppear = false
    @State private var showingMismatchReview = false
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

    @ViewBuilder
    private var rootContent: some View {
        #if DEBUG
            // Always show the list in Debug builds so the Debug TTY section
            // remains reachable even when no hosts have been added yet.
            hostsList
        #else
            if viewModel.entries.isEmpty && !viewModel.loadFailed {
                emptyState
            } else {
                hostsList
            }
        #endif
    }

    private var hostsList: some View {
        ScrollView {
            LazyVStack(spacing: 12) {
                #if DEBUG
                    DebugTTYSection(path: $path)
                        .padding(.top, 8)
                #endif
                ForEach(viewModel.entries) { entry in
                    HostRow(
                        entry: entry,
                        inFlight: viewModel.inFlightID == entry.id,
                        isCurrentSession: viewModel.currentSessionID == entry.id,
                        onEdit: { path.append(AppRoute.hostForm(entry.id)) },
                        onConnect: {
                            viewModel.onConnectError = handleConnectError
                            viewModel.requestConnect(id: entry.id)
                        }
                    )
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
            if let session = viewModel.lastSession, let entry = currentSessionEntry() {
                TerminalScreen(
                    session: session,
                    credential: entry.credential,
                    onExit: {
                        let label = viewModel.displayName(for: entry.id)
                        viewModel.sessionEnded()
                        path = NavigationPath()
                        toaster.show(
                            .info,
                            title: String(localized: "Session ended"),
                            description: String(localized: "Disconnected from \(label).")
                        )
                    }
                )
            } else {
                Text("No session.")
            }
        #if DEBUG
            case .debugTerminal(let selection):
                debugTerminalScreen(for: selection)
        #endif
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

    private func handleConnectError(id: UUID, message: String, permissionDenied: Bool) {
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
