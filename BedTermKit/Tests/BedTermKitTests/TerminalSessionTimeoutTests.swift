import Foundation
import Testing

@testable import BedTermKit

@Suite("TerminalSession connect timeout")
@MainActor
struct TerminalSessionTimeoutTests {
    @Test
    func hangingConnectTimesOutAndSurfacesClosedState() async throws {
        let client = MockSSHClient()
        client.scriptConnectHang()

        let session = TerminalSession(
            client: client,
            hostID: UUID(),
            persistence: nil
        )

        await session.connect(
            credential: HostCredential(
                host: "10.255.255.1", port: 22, username: "test",
                auth: .password("x")
            ),
            initialPTY: .init(cols: 80, rows: 24),
            timeout: 0.05
        )

        guard case .closed(let reason) = session.state else {
            Issue.record("Expected .closed after timeout, got \(session.state)")
            return
        }
        #expect(session.lastError == .timeout)
        #expect(reason == String(localized: "Connection timed out."))
    }
}
