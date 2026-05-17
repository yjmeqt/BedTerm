import SwiftUI

// Confirmation dialog modifiers for `HostsScreen`. Lifted to a sibling file so
// the screen stays under the SwiftLint file-length cap.

struct SwapDialogModifier: ViewModifier {
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

struct DeleteDialogModifier: ViewModifier {
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
                Text(
                    "You are currently connected. The session will end and the saved password or key will be removed."
                )
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
