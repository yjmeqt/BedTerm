import Foundation
import Testing

@testable import BedTermKit

@MainActor
@Suite("ShellIntegration")
struct ShellIntegrationTests {
    // MARK: - Resource loading

    @Test("embedded payload loads via Rust FFI")
    func scriptResourceLoads() {
        // The bedterm-integration.sh body is embedded into the bedterm-ios
        // staticlib at build time via `include_bytes!`. If `load()` returns
        // nil, the staticlib was built from a missing or non-UTF-8 source
        // file — a build accident, not a runtime concern.
        let body = ShellIntegrationScript.load()
        #expect(body != nil)
        #expect((body?.count ?? 0) > 0)
    }

    @Test("script emits the DCS sequences the parser expects")
    func scriptEmitsDcsSequences() throws {
        let body = try #require(ShellIntegrationScript.load())
        // The DCS opener — `ESC P $ d` — that wraps every hex-encoded JSON
        // payload must be present as a literal printf format string.
        #expect(body.contains(#"\033P$d"#))
        // Warp-tagged JSON shapes for the three hook variants we ship.
        #expect(body.contains(#"{"hook":"Precmd","value":{"pwd":"#))
        #expect(body.contains(#"{"hook":"Preexec","value":{"command":"#))
        #expect(body.contains(#"{"hook":"CommandFinished","value":{"exit_code":"#))
        // The zsh + bash hook entry points.
        #expect(body.contains("__bedterm_precmd"))
        #expect(body.contains("__bedterm_preexec"))
        // Idempotence guard so re-source doesn't double-install hooks.
        #expect(body.contains("__BEDTERM_INTEGRATION_INSTALLED"))
    }

    // MARK: - Bootstrap payload

    @Test("bootstrap payload matches raw script body")
    func bootstrapPayloadMatchesRawScriptBody() throws {
        // RusshSSHClient injects the bootstrap payload after the first
        // remote shell byte, so bootstrapPayload must equal the raw script
        // body returned by load().
        let body = try #require(ShellIntegrationScript.load())
        let payload = try #require(ShellIntegrationScript.bootstrapPayload())
        #expect(payload == body)
    }

    // MARK: - End-to-end: bootstrap flows through connect

    @Test("connect propagates bootstrap payload to client")
    func connectPropagatesBootstrapPayloadToClient() async throws {
        let mock = MockSSHClient()
        let session = TerminalSession(client: mock)
        let credential = HostCredential(host: "test.example.com", port: 22, username: "alice", auth: .password(""))
        let payload = try #require(ShellIntegrationScript.bootstrapPayload())
        await session.connect(
            credential: credential,
            initialPTY: .init(cols: 80, rows: 24),
            bootstrapPayload: payload
        )
        let captured = try #require(mock.lastConnectRequest)
        #expect(captured.bootstrapPayload == payload)
    }

    @Test("connect omits bootstrap when not provided")
    func connectOmitsBootstrapWhenNotProvided() async {
        let mock = MockSSHClient()
        let session = TerminalSession(client: mock)
        let credential = HostCredential(host: "test.example.com", port: 22, username: "alice", auth: .password(""))
        await session.connect(
            credential: credential,
            initialPTY: .init(cols: 80, rows: 24)
        )
        #expect(mock.lastConnectRequest?.bootstrapPayload == nil)
    }

}
