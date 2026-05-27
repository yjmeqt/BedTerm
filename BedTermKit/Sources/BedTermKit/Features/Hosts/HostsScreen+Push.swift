import SwiftUI
import UIKit

extension HostsScreen {
    /// Asks the coordinator to push the connection form. Wraps the
    /// form-outcome wiring so the screen owns the post-form behaviour
    /// (refresh, toast, optional connect kick-off) regardless of which
    /// nav layer surfaces it.
    func presentHostForm(id: SavedHost.ID?) {
        let connectOnSave = pendingConnectOnSave && id == nil
        onShowHostForm(id, connectOnSave) { outcome in
            handleFormOutcome(outcome)
        }
    }

    /// Build a terminal VC for the current live session and ask the
    /// coordinator to push it. Pops back to Hosts when the user taps
    /// back or kill.
    func pushTerminal() {
        guard let session = viewModel.lastSession, let entry = currentSessionEntry() else {
            return
        }
        let hostName = viewModel.displayName(for: entry.id)
        let entryID = entry.id
        let payloadProvider: @MainActor () -> String? = { [settings] in
            guard settings.showCommandBlocks else { return nil }
            return ShellIntegrationScript.bootstrapPayload()
        }
        let popHandler = onPopToHosts
        let onBack: () -> Void = { popHandler() }
        let onKill: () -> Void = {
            handleTerminalKill(entryID: entryID)
            popHandler()
        }
        let vc = TerminalScreenViewController(
            session: session,
            credential: entry.credential,
            hostName: hostName,
            bootstrapPayloadProvider: payloadProvider,
            onBack: onBack,
            onKill: onKill
        )
        onShowTerminal(vc)
    }

    func currentSessionEntry() -> SavedHost? {
        guard let id = viewModel.currentSessionID else { return nil }
        return viewModel.entries.first { $0.id == id }
    }

    /// User tapped × on the terminal toolbar: tear down the live session.
    /// Coordinator handles popping back to Hosts when this fires.
    func handleTerminalKill(entryID: UUID) {
        let label = viewModel.displayName(for: entryID)
        viewModel.endLiveSession()
        toaster.show(
            .info,
            title: String(localized: "Session ended"),
            description: String(localized: "Disconnected from \(label).")
        )
    }

    func handleFormOutcome(_ outcome: ConnectionFormScreen.Outcome) {
        viewModel.refresh()
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
}
