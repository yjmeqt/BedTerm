import UIKit

// Swap + delete confirmation alerts for `HostsConnectController`.
// Lifted to a sibling file so the controller stays under the
// SwiftLint file-length cap (mirrors the old
// `HostsScreenDialogs.swift` pattern).

extension HostsConnectController {
    func handleSwapConfirmationChanged() {
        // Dismiss any stale alert when the VM clears its confirmation;
        // only re-present when a new target appears.
        if self.viewModel.swapConfirmation == nil {
            self.presentedSwapAlert?.dismiss(animated: true)
            self.presentedSwapAlert = nil
            return
        }
        guard self.presentedSwapAlert == nil, let target = self.viewModel.swapConfirmation
        else { return }
        let alert = UIAlertController(
            title: String(
                localized: "End current session and connect to \"\(target.displayName)\"?"),
            message: String(localized: "Your current SSH session will be disconnected."),
            preferredStyle: .alert
        )
        alert.addAction(
            UIAlertAction(title: String(localized: "Connect"), style: .default) { [weak self] _ in
                self?.viewModel.confirmSwap()
            })
        alert.addAction(
            UIAlertAction(title: String(localized: "Cancel"), style: .cancel) { [weak self] _ in
                self?.viewModel.cancelSwap()
            })
        self.presentedSwapAlert = alert
        self.rootViewController.present(alert, animated: true)
    }

    func handleDeleteConfirmationChanged() {
        if self.viewModel.deleteConfirmation == nil {
            self.presentedDeleteAlert?.dismiss(animated: true)
            self.presentedDeleteAlert = nil
            return
        }
        guard self.presentedDeleteAlert == nil, let target = self.viewModel.deleteConfirmation
        else { return }
        let title: String
        if target.isLive {
            title = String(localized: "Disconnect and delete \"\(target.displayName)\"?")
        } else {
            title = String(localized: "Delete \"\(target.displayName)\"?")
        }
        let message: String
        if target.isLive {
            message = String(
                localized:
                    "You are currently connected. The session will end and the saved password or key will be removed."
            )
        } else {
            message = String(localized: "This will remove the saved password or key.")
        }
        let alert = UIAlertController(title: title, message: message, preferredStyle: .alert)
        alert.addAction(
            UIAlertAction(title: String(localized: "Delete"), style: .destructive) { [weak self] _ in
                self?.viewModel.confirmDelete()
            })
        alert.addAction(
            UIAlertAction(title: String(localized: "Cancel"), style: .cancel) { [weak self] _ in
                self?.viewModel.cancelDelete()
            })
        self.presentedDeleteAlert = alert
        self.rootViewController.present(alert, animated: true)
    }
}
