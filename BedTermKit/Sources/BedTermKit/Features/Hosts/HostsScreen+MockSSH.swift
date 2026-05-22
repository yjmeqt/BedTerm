#if DEBUG
    import SwiftUI

    extension HostsScreen {
        /// Hard-coded credentials for the loopback `bedterm-mock-ssh`
        /// server. Drops any previously-stored host fingerprint so a
        /// fresh key lands as trust-on-first-use instead of a
        /// "Host key changed" dead-end.
        func mockSSHCredential() -> HostCredential {
            let credential = HostCredential(
                host: "127.0.0.1", port: 2222, username: "test",
                auth: .password("x"))
            HostKeyStore().remove(host: credential.host, port: credential.port)
            return credential
        }

        @ViewBuilder
        var mockSSHDestination: some View {
            TerminalScreen(
                mockSSHClient: CitadelSSHClient(),
                credential: mockSSHCredential()
            ) {
                if !path.isEmpty { path.removeLast() }
            }
        }

        /// Ephemeral debug-only host row that pushes `AppRoute.mockSSH`.
        /// Not persisted — only exists in the running DEBUG app.
        @ViewBuilder
        var debugMockSSHRow: some View {
            let credential = mockSSHCredential()
            let id =
                UUID(uuidString: "0000DEBC-0001-0000-0000-000027C2DD22")
                ?? UUID()
            let entry = SavedHost(
                id: id, label: "Mock SSH (loopback)", credential: credential)
            HostRow(
                entry: entry,
                inFlight: false,
                isCurrentSession: false,
                onTapBody: { path.append(AppRoute.mockSSH) },
                onConnect: { path.append(AppRoute.mockSSH) }
            )
        }
    }
#endif
