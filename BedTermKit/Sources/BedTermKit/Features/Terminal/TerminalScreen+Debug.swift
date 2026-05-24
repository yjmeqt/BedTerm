#if DEBUG
    import SwiftUI

    extension TerminalScreen {
        /// Debug-only: build a TerminalScreen against a caller-supplied SSH
        /// client (e.g. CitadelSSHClient pointed at the loopback
        /// `bedterm-mock-ssh` server). Bypasses the hosts list / view model.
        init(
            mockSSHClient: any SSHClient,
            credential: HostCredential,
            onExit: @escaping () -> Void
        ) {
            let session = TerminalSession(client: mockSSHClient, hostID: UUID(), persistence: nil)
            self.init(
                session: session,
                credential: credential,
                hostName: "mock-ssh",
                onBack: onExit,
                onKill: { _ in onExit() }
            )
        }
    }
#endif
