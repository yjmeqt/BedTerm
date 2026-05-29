import BedTermIOS
import UIKit

// Swap + delete confirmation alerts for `HostsConnectController`.
// Lifted to a sibling file to keep the controller focused.
//
// All alert text — title / message / button labels — comes from Rust
// (`bt_ios_hosts_vm_*_alert`). Swift owns presentation lifecycle only;
// it neither localizes nor interpolates the host display name. Add new
// copy in `BedTerm/Localizable.xcstrings` + `hosts_vm::*_alert`, never
// here.

extension HostsConnectController {
    func handleSwapConfirmationChanged() {
        // Dismiss any stale alert when the VM clears its confirmation;
        // only re-present when a new target appears.
        if self.viewModel.swapConfirmation == nil {
            self.presentedSwapAlert?.dismiss(animated: true)
            self.presentedSwapAlert = nil
            return
        }
        guard self.presentedSwapAlert == nil else { return }
        guard let text = Self.readAlertText(bt_ios_hosts_vm_swap_alert()) else { return }
        let alert = UIAlertController(
            title: text.title, message: text.message, preferredStyle: .alert)
        alert.addAction(
            UIAlertAction(title: text.confirmLabel, style: .default) { [weak self] _ in
                self?.viewModel.confirmSwap()
            })
        alert.addAction(
            UIAlertAction(title: text.cancelLabel, style: .cancel) { [weak self] _ in
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
        guard self.presentedDeleteAlert == nil else { return }
        guard let text = Self.readAlertText(bt_ios_hosts_vm_delete_alert()) else { return }
        let alert = UIAlertController(
            title: text.title, message: text.message, preferredStyle: .alert)
        alert.addAction(
            UIAlertAction(title: text.confirmLabel, style: .destructive) { [weak self] _ in
                self?.viewModel.confirmDelete()
            })
        alert.addAction(
            UIAlertAction(title: text.cancelLabel, style: .cancel) { [weak self] _ in
                self?.viewModel.cancelDelete()
            })
        self.presentedDeleteAlert = alert
        self.rootViewController.present(alert, animated: true)
    }

    /// Plain-Swift mirror of `BtIosHostsAlertText`. The Rust side owns the
    /// underlying C string buffers; we copy each field into a Swift
    /// `String` and immediately free the struct, so callers never have to
    /// juggle lifetimes.
    struct AlertText {
        let title: String
        let message: String
        let confirmLabel: String
        let cancelLabel: String
    }

    static func readAlertText(_ ptr: UnsafeMutablePointer<BtIosHostsAlertText>?) -> AlertText? {
        guard let ptr else { return nil }
        defer { bt_ios_hosts_free_alert_text(ptr) }
        let raw = ptr.pointee
        return AlertText(
            title: String(cString: raw.title),
            message: String(cString: raw.message),
            confirmLabel: String(cString: raw.confirm_label),
            cancelLabel: String(cString: raw.cancel_label)
        )
    }
}
