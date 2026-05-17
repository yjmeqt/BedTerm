import SwiftUI
import UIKit

public struct ConnectionFormScreen: View {
    /// What the form's primary action does on success.
    public enum Outcome {
        /// User saved the entry. The host's id is provided so the caller can
        /// scroll it into view on the list.
        case saved(SavedHost.ID)
        /// User saved the entry AND wants to start a connect to it immediately.
        /// Returned only when the screen was opened in `connectOnSave` mode
        /// (the first-run shortcut).
        case savedAndConnect(SavedHost.ID)
        /// User cancelled.
        case cancelled
    }

    @Environment(\.dismiss) private var dismiss
    @State private var viewModel: ConnectionFormViewModel
    @State private var keyImporter = false
    @State private var duplicateLabel: String?
    #if DEBUG
        @AppStorage("debug.useMetalRenderer") private var useMetalRenderer: Bool = false
    #endif

    let connectOnSave: Bool
    let onFinish: (Outcome) -> Void

    public init(
        editingID: SavedHost.ID?,
        store: HostsStore = HostsStore(),
        connectOnSave: Bool = false,
        onFinish: @escaping (Outcome) -> Void
    ) {
        self.connectOnSave = connectOnSave
        self.onFinish = onFinish
        let mode: ConnectionFormViewModel.Mode
        if let id = editingID, let existing = try? store.load(id: id) {
            mode = .edit(existing)
        } else {
            mode = .add
        }
        self._viewModel = State(initialValue: ConnectionFormViewModel(mode: mode, store: store))
    }

    public var body: some View {
        ScrollView {
            VStack(spacing: 12) {
                connectionCard
                authenticationCard
                if let dup = duplicateLabel {
                    duplicateAlert(label: dup)
                }
                if let error = viewModel.errorMessage {
                    errorAlert(message: error)
                }
                #if DEBUG
                    debugCard
                #endif
            }
            .padding(16)
        }
        .background(Color("ShadcnBackground", bundle: .module).ignoresSafeArea())
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button(String(localized: "Cancel")) {
                    onFinish(.cancelled)
                    dismiss()
                }
                .accessibilityIdentifier("connection.cancel")
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button(primaryActionTitle) { onSave() }
                    .disabled(!viewModel.canSave)
                    .fontWeight(.semibold)
                    .accessibilityIdentifier("connection.save")
            }
        }
        .fileImporter(isPresented: $keyImporter, allowedContentTypes: [.data]) { result in
            if case .success(let url) = result, let data = try? Data(contentsOf: url) {
                viewModel.privateKey = data
                viewModel.privateKeyTouched = true
            }
        }
        .onChange(of: viewModel.host) { _, _ in recomputeDuplicate() }
        .onChange(of: viewModel.port) { _, _ in recomputeDuplicate() }
        .onChange(of: viewModel.username) { _, _ in recomputeDuplicate() }
        .onAppear { recomputeDuplicate() }
    }

    private var title: String {
        switch viewModel.mode {
        case .add: return String(localized: "New Host")
        case .edit: return String(localized: "Edit Host")
        }
    }

    private var primaryActionTitle: String {
        if connectOnSave { return String(localized: "Save & Connect") }
        return String(localized: "Save")
    }

    // MARK: - Cards

    private var connectionCard: some View {
        ShadcnCard(
            title: String(localized: "Connection"),
            description: String(localized: "Where to reach the server. Host can be an IP or hostname.")
        ) {
            ShadcnField(label: String(localized: "Label")) {
                ShadcnTextField(text: $viewModel.label, identifier: "connection.label")
            }
            ShadcnField(label: String(localized: "Host")) {
                ShadcnTextField(text: $viewModel.host, identifier: "connection.host")
            }
            ShadcnField(label: String(localized: "Port")) {
                ShadcnTextField(
                    text: $viewModel.port,
                    keyboard: .numberPad,
                    identifier: "connection.port"
                )
            }
            ShadcnField(label: String(localized: "Username")) {
                ShadcnTextField(text: $viewModel.username, identifier: "connection.username")
            }
        }
    }

    private var authenticationCard: some View {
        ShadcnCard(
            title: String(localized: "Authentication"),
            description: String(localized: "Choose how to prove identity to the server.")
        ) {
            ShadcnSegmented(selection: $viewModel.auth)

            if viewModel.auth == .password {
                passwordField
            } else {
                privateKeyFields
            }
        }
    }

    #if DEBUG
        private var debugCard: some View {
            ShadcnCard(title: String(localized: "Debug"), description: nil) {
                Toggle("Metal renderer (experimental)", isOn: $useMetalRenderer)
                    .toggleStyle(.switch)
            }
        }
    #endif

    @ViewBuilder
    private var passwordField: some View {
        ShadcnField(label: String(localized: "Password")) {
            if case .edit = viewModel.mode, !viewModel.passwordTouched {
                ShadcnSecureField(
                    text: $viewModel.password,
                    placeholder: String(localized: "Stored — replace?")
                )
                .onChange(of: viewModel.password) { _, new in
                    if !new.isEmpty { viewModel.passwordTouched = true }
                }
            } else {
                ShadcnSecureField(
                    text: $viewModel.password,
                    placeholder: String(localized: "Password"),
                    contentType: .password,
                    identifier: "connection.password"
                )
                .onChange(of: viewModel.password) { _, _ in viewModel.passwordTouched = true }
            }
        }
    }

    @ViewBuilder
    private var privateKeyFields: some View {
        ShadcnField(label: String(localized: "Private key")) {
            Button {
                keyImporter = true
            } label: {
                HStack {
                    Text(keyButtonLabel)
                        .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                    Spacer()
                    Image(systemName: "chevron.right")
                        .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        .font(.footnote.weight(.semibold))
                }
                .padding(.horizontal, 12)
                .frame(height: 36)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(Color("ShadcnInput", bundle: .module), lineWidth: 1)
                )
            }
            .buttonStyle(.plain)
        }
        ShadcnField(
            label: String(localized: "Passphrase"),
            help: String(localized: "Optional")
        ) {
            if case .edit = viewModel.mode, !viewModel.passphraseTouched {
                ShadcnSecureField(
                    text: $viewModel.passphrase,
                    placeholder: String(localized: "Stored — replace?")
                )
                .onChange(of: viewModel.passphrase) { _, new in
                    if !new.isEmpty { viewModel.passphraseTouched = true }
                }
            } else {
                ShadcnSecureField(
                    text: $viewModel.passphrase,
                    placeholder: String(localized: "Passphrase (optional)")
                )
                .onChange(of: viewModel.passphrase) { _, _ in viewModel.passphraseTouched = true }
            }
        }
    }

    private var keyButtonLabel: String {
        if viewModel.privateKey != nil {
            return viewModel.privateKeyTouched
                ? String(localized: "Replace private key")
                : String(localized: "Stored — replace?")
        }
        if case .edit = viewModel.mode, viewModel.auth == .privateKey, !viewModel.privateKeyTouched {
            return String(localized: "Stored — replace?")
        }
        return String(localized: "Import private key")
    }

    // MARK: - Alerts

    private func duplicateAlert(label: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
                .font(.footnote)
            VStack(alignment: .leading, spacing: 4) {
                Text("Duplicate host")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                Text("You already have a saved host for this user@host:port (\"\(label)\"). Save anyway?")
                    .font(.footnote)
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color("ShadcnCard", bundle: .module))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    private func errorAlert(message: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.circle.fill")
                .foregroundStyle(Color("ShadcnDestructive", bundle: .module))
                .font(.footnote)
            Text(message)
                .font(.footnote)
                .foregroundStyle(Color("ShadcnDestructive", bundle: .module))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("connection.error")
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color("ShadcnDestructive", bundle: .module).opacity(0.08))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color("ShadcnDestructive", bundle: .module).opacity(0.3), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    // MARK: - Actions

    private func onSave() {
        do {
            let id = try viewModel.save()
            onFinish(connectOnSave ? .savedAndConnect(id) : .saved(id))
            dismiss()
        } catch {
            // viewModel.errorMessage is set; nothing else to do.
        }
    }

    private func recomputeDuplicate() {
        duplicateLabel = viewModel.duplicate()?.label
    }
}

private struct ShadcnSegmented: View {
    @Binding var selection: ConnectionFormViewModel.AuthChoice

    var body: some View {
        HStack(spacing: 0) {
            tab(title: String(localized: "Password"), value: .password)
            tab(title: String(localized: "Key"), value: .privateKey)
        }
        .padding(3)
        .background(Color("ShadcnBackground", bundle: .module))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func tab(title: String, value: ConnectionFormViewModel.AuthChoice) -> some View {
        let isActive = selection == value
        return Button {
            selection = value
        } label: {
            Text(title)
                .font(.footnote.weight(.medium))
                .foregroundStyle(
                    isActive
                        ? Color("ShadcnPrimary", bundle: .module)
                        : Color("ShadcnMutedForeground", bundle: .module)
                )
                .frame(maxWidth: .infinity)
                .frame(height: 28)
                .background(
                    isActive
                        ? Color("ShadcnCard", bundle: .module)
                        : Color.clear
                )
                .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
    }
}
