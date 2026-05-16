import Foundation
import Testing

@testable import BedTerm

@Suite("TerminalSession.describe")
struct TerminalSessionDescribeTests {
    @Test(
        "each SSHError maps to a non-empty user-facing string",
        arguments: [
            SSHError.dnsResolution,
            .tcpRefused,
            .timeout,
            .handshakeFailed("kex"),
            .authenticationFailed,
            .privateKeyParse,
            .privateKeyPassphraseRequired,
            .hostKeyMismatch(stored: "SHA256:a", remote: "SHA256:b"),
            .disconnected("eof"),
            .shellExited(1)
        ])
    func describesEverything(_ error: SSHError) {
        let text = TerminalSession.describe(error)
        #expect(!text.isEmpty)
    }
}
