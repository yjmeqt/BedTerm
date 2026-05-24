import SwiftUI

extension HostsScreen {
    @ViewBuilder
    var terminalDestination: some View {
        if let session = viewModel.lastSession, let entry = currentSessionEntry() {
            let hostName = viewModel.displayName(for: entry.id)
            if settings.useRustTerminal {
                RsTerminalView(onBack: handleTerminalBack)
                    .ignoresSafeArea(.keyboard, edges: .bottom)
                    .navigationTitle(hostName)
                    .navigationBarTitleDisplayMode(.inline)
            } else {
                TerminalScreen(
                    session: session,
                    credential: entry.credential,
                    hostName: hostName,
                    onBack: {
                        handleTerminalBack()
                    },
                    onKill: { reason in
                        handleTerminalKill(entryID: entry.id, reason: reason)
                    }
                )
            }
        } else {
            Text("No session.")
        }
    }

    /// Leading Back chevron: pop the terminal screen but leave the
    /// session in the view model so re-entering from the Hosts list
    /// (inline session row or sessions panel) returns to the same
    /// running `TerminalSession` (PRD R3.back_button). No toast, no
    /// snapshot — the session is still considered running.
    func handleTerminalBack() {
        if !path.isEmpty { path.removeLast() }
    }

    func handleTerminalKill(entryID: UUID, reason: SessionSnapshot.KillReason) {
        let label = viewModel.displayName(for: entryID)
        viewModel.snapshotAndEnd(reason: reason)
        // Pop to the panel so the new killed-session entry is
        // immediately visible (PRD R3.kill_button).
        path = NavigationPath()
        path.append(AppRoute.sessionsPanel(entryID))
        toaster.show(
            .info,
            title: String(localized: "Session ended"),
            description: String(localized: "Disconnected from \(label).")
        )
    }

    @ViewBuilder
    func sessionsPanelDestination(hostID: UUID) -> some View {
        if let host = viewModel.entries.first(where: { $0.id == hostID }) {
            SessionsPanelScreen(
                path: $path,
                host: host,
                onStartNew: {
                    viewModel.onConnectError = handleConnectError
                    viewModel.requestConnect(id: hostID)
                }
            )
        } else {
            Text("Host not found.")
        }
    }

    @ViewBuilder
    func killedSessionDetailDestination(snapshotID: UUID) -> some View {
        if let host = resolveKilledSessionHost(snapshotID: snapshotID) {
            KilledSessionDetailScreen(
                path: $path,
                snapshotID: snapshotID,
                host: host,
                onResume: { initialInput in
                    viewModel.onConnectError = handleConnectError
                    viewModel.requestResume(hostID: host.id, initialCommand: initialInput)
                }
            )
        } else {
            Text("Session no longer available.")
        }
    }

    private func resolveKilledSessionHost(snapshotID: UUID) -> SavedHost? {
        guard let snapshot = snapshotStore.snapshot(id: snapshotID) else { return nil }
        return viewModel.entries.first { $0.id == snapshot.hostID }
    }
}
