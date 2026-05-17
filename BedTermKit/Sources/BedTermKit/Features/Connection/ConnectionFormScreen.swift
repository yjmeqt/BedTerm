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
        Form {
            Section {
                LabeledField(
                    label: String(localized: "Label"),
                    text: $viewModel.label,
                    identifier: "connection.label"
                )
                LabeledField(
                    label: String(localized: "Host"),
                    text: $viewModel.host,
                    monospaced: true,
                    identifier: "connection.host"
                )
                LabeledField(
                    label: String(localized: "Port"),
                    text: $viewModel.port,
                    monospaced: true,
                    keyboard: .numberPad,
                    identifier: "connection.port"
                )
                LabeledField(
                    label: String(localized: "Username"),
                    text: $viewModel.username,
                    monospaced: true,
                    identifier: "connection.username"
                )
            }
            Section {
                Picker(String(localized: "Auth method"), selection: $viewModel.auth) {
                    Text("Password").tag(ConnectionFormViewModel.AuthChoice.password)
                    Text("Key").tag(ConnectionFormViewModel.AuthChoice.privateKey)
                }
                .pickerStyle(.segmented)

                if viewModel.auth == .password {
                    passwordField
                } else {
                    privateKeyFields
                }
            }
            if let dup = duplicateLabel {
                Section {
                    HStack(alignment: .top, spacing: 8) {
                        Text("warn")
                            .font(.caption2.weight(.semibold))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Color.orange, in: RoundedRectangle(cornerRadius: 4))
                            .foregroundStyle(.white)
                        Text("You already have a saved host for this user@host:port (\"\(dup)\"). Save anyway?")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            if let error = viewModel.errorMessage {
                Section {
                    Text(error).foregroundStyle(.red)
                        .accessibilityIdentifier("connection.error")
                }
            }
        }
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

    @ViewBuilder
    private var passwordField: some View {
        if case .edit = viewModel.mode, !viewModel.passwordTouched {
            SecureField(String(localized: "Stored — replace?"), text: $viewModel.password)
                .onChange(of: viewModel.password) { _, new in
                    if !new.isEmpty { viewModel.passwordTouched = true }
                }
        } else {
            SecureField(String(localized: "Password"), text: $viewModel.password)
                .textContentType(.password)
                .accessibilityIdentifier("connection.password")
                .onChange(of: viewModel.password) { _, _ in viewModel.passwordTouched = true }
        }
    }

    @ViewBuilder
    private var privateKeyFields: some View {
        Button {
            keyImporter = true
        } label: {
            HStack {
                Text(keyButtonLabel)
                Spacer()
                Image(systemName: "chevron.right").foregroundStyle(.tertiary)
            }
        }
        if case .edit = viewModel.mode, !viewModel.passphraseTouched {
            SecureField(String(localized: "Stored — replace?"), text: $viewModel.passphrase)
                .onChange(of: viewModel.passphrase) { _, new in
                    if !new.isEmpty { viewModel.passphraseTouched = true }
                }
        } else {
            SecureField(String(localized: "Passphrase (optional)"), text: $viewModel.passphrase)
                .onChange(of: viewModel.passphrase) { _, _ in viewModel.passphraseTouched = true }
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

private struct LabeledField: View {
    let label: String
    @Binding var text: String
    var monospaced: Bool = false
    var keyboard: UIKeyboardType = .default
    var identifier: String?

    var body: some View {
        HStack {
            Text(label)
                .foregroundStyle(.primary)
                .frame(width: 110, alignment: .leading)
            TextField("", text: $text)
                .multilineTextAlignment(.trailing)
                .keyboardType(keyboard)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .modifier(MonospacedIf(enabled: monospaced))
                .modifier(IdentifierIf(identifier: identifier))
        }
    }
}

private struct IdentifierIf: ViewModifier {
    let identifier: String?
    func body(content: Content) -> some View {
        if let id = identifier { content.accessibilityIdentifier(id) } else { content }
    }
}

private struct MonospacedIf: ViewModifier {
    let enabled: Bool
    func body(content: Content) -> some View {
        if enabled { content.monospaced() } else { content }
    }
}
