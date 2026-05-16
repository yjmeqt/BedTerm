import SwiftUI
import UIKit

public struct ConnectionScreen: View {
    @Binding var path: NavigationPath
    @State private var viewModel = ConnectionViewModel(clientFactory: { CitadelSSHClient() })
    @State private var keyImporter = false

    public init(path: Binding<NavigationPath>) {
        self._path = path
    }

    public var body: some View {
        Form {
            Section("Server") {
                TextField("Host", text: $viewModel.host)
                    .textContentType(.URL)
                    .keyboardType(.URL)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .accessibilityIdentifier("connection.host")
                TextField("Port", text: $viewModel.port)
                    .keyboardType(.numberPad)
                    .accessibilityIdentifier("connection.port")
                TextField("Username", text: $viewModel.username)
                    .textContentType(.username)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .accessibilityIdentifier("connection.username")
            }

            Section("Authentication") {
                Picker("Method", selection: $viewModel.auth) {
                    Text("Password").tag(ConnectionViewModel.AuthChoice.password)
                    Text("Private Key").tag(ConnectionViewModel.AuthChoice.privateKey)
                }
                .pickerStyle(.segmented)

                if viewModel.auth == .password {
                    SecureField("Password", text: $viewModel.password)
                        .textContentType(.password)
                        .accessibilityIdentifier("connection.password")
                } else {
                    Button(viewModel.privateKey == nil ? "Import private key" : "Replace private key") {
                        keyImporter = true
                    }
                    SecureField("Passphrase (optional)", text: $viewModel.passphrase)
                }
            }

            if let error = viewModel.errorMessage {
                Section {
                    Text(error)
                        .foregroundStyle(.red)
                        .accessibilityIdentifier("connection.error")
                    if viewModel.permissionDenied {
                        Button("Open Settings") {
                            if let url = URL(string: UIApplication.openSettingsURLString) {
                                UIApplication.shared.open(url)
                            }
                        }
                        .accessibilityIdentifier("connection.openSettings")
                        Button("Retry") {
                            Task { await attemptConnect() }
                        }
                        .accessibilityIdentifier("connection.retry")
                    }
                }
            }

            Section {
                Button {
                    Task { await attemptConnect() }
                } label: {
                    if viewModel.isPrewarming {
                        HStack {
                            ProgressView()
                            Text("Waiting for local network permission…")
                        }
                        .frame(maxWidth: .infinity)
                    } else if viewModel.isConnecting {
                        ProgressView()
                            .frame(maxWidth: .infinity)
                    } else {
                        Text("Connect")
                            .frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(viewModel.isConnecting)
                .accessibilityIdentifier("connection.connect")
            }
        }
        .navigationTitle("BedTerm")
        .navigationDestination(for: AppRoute.self) { route in
            switch route {
            case .terminal:
                if let session = viewModel.lastSession {
                    TerminalScreen(
                        session: session,
                        credential: viewModel.buildCredential() ?? .placeholder,
                        onExit: { path = NavigationPath() }
                    )
                } else {
                    Text("No session.")
                }
            case .hostKeyMismatch(let stored, let remote, let host, let port):
                HostKeyMismatchScreen(
                    stored: stored, remote: remote, host: host, port: port,
                    onTrust: { Task { await attemptConnect() } },
                    onReject: { path = NavigationPath() }
                )
            }
        }
        .fileImporter(
            isPresented: $keyImporter,
            allowedContentTypes: [.data]
        ) { result in
            if case .success(let url) = result, let data = try? Data(contentsOf: url) {
                viewModel.privateKey = data
            }
        }
        .task { viewModel.loadSaved() }
    }

    private func attemptConnect() async {
        if let session = await viewModel.connect() {
            _ = session
            path.append(AppRoute.terminal)
        } else if let mismatch = viewModel.pendingMismatch {
            path.append(
                AppRoute.hostKeyMismatch(
                    stored: mismatch.stored,
                    remote: mismatch.remote,
                    host: mismatch.host,
                    port: mismatch.port
                )
            )
        }
    }
}

extension HostCredential {
    static let placeholder = HostCredential(host: "", port: 22, username: "", auth: .password(""))
}
