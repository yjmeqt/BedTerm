import XCTest

@testable import BedTermKit

@MainActor
final class ShellIntegrationTests: XCTestCase {
    // MARK: - Resource loading

    func testScriptResourceLoads() {
        // The bundled bedterm-integration.sh must always be reachable via
        // Bundle.module. If this fails, the resource was dropped from the
        // SPM target — a packaging accident, not a runtime concern.
        XCTAssertNotNil(ShellIntegrationScript.load())
    }

    func testScriptEmitsDcsSequences() throws {
        let body = try XCTUnwrap(ShellIntegrationScript.load())
        // The DCS opener — `ESC P $ d` — that wraps every hex-encoded JSON
        // payload must be present as a literal printf format string.
        XCTAssertTrue(body.contains(#"\033P$d"#))
        // Warp-tagged JSON shapes for the three hook variants we ship.
        XCTAssertTrue(body.contains(#"{"hook":"Precmd","value":{"pwd":"#))
        XCTAssertTrue(body.contains(#"{"hook":"Preexec","value":{"command":"#))
        XCTAssertTrue(body.contains(#"{"hook":"CommandFinished","value":{"exit_code":"#))
        // The zsh + bash hook entry points.
        XCTAssertTrue(body.contains("__bedterm_precmd"))
        XCTAssertTrue(body.contains("__bedterm_preexec"))
        // Idempotence guard so re-source doesn't double-install hooks.
        XCTAssertTrue(body.contains("__BEDTERM_INTEGRATION_INSTALLED"))
    }

    // MARK: - Bootstrap payload

    func testBootstrapPayloadMatchesRawScriptBody() throws {
        // The SFTP path in CitadelSSHClient+Bootstrap writes the bootstrap
        // payload verbatim to ~/.cache/bedterm/integration.sh — no heredoc
        // wrapping, no HISTCONTROL prefix. So bootstrapPayload must equal
        // the raw script body returned by load().
        let body = try XCTUnwrap(ShellIntegrationScript.load())
        let payload = try XCTUnwrap(ShellIntegrationScript.bootstrapPayload())
        XCTAssertEqual(payload, body)
    }

    // MARK: - End-to-end: bootstrap flows through connect

    func testConnectPropagatesBootstrapPayloadToClient() async throws {
        let mock = MockSSHClient()
        let session = TerminalSession(client: mock)
        let credential = HostCredential(host: "test.example.com", port: 22, username: "alice", auth: .password(""))
        let payload = try XCTUnwrap(ShellIntegrationScript.bootstrapPayload())
        await session.connect(
            credential: credential,
            initialPTY: .init(cols: 80, rows: 24),
            bootstrapPayload: payload
        )
        let captured = try XCTUnwrap(mock.lastConnectRequest)
        XCTAssertEqual(captured.bootstrapPayload, payload)
    }

    func testConnectOmitsBootstrapWhenNotProvided() async {
        let mock = MockSSHClient()
        let session = TerminalSession(client: mock)
        let credential = HostCredential(host: "test.example.com", port: 22, username: "alice", auth: .password(""))
        await session.connect(
            credential: credential,
            initialPTY: .init(cols: 80, rows: 24)
        )
        XCTAssertNil(mock.lastConnectRequest?.bootstrapPayload)
    }

    // MARK: - Settings gating

    func testSettingsToggleControlsBootstrap() throws {
        let suite = UUID().uuidString
        UserDefaults().removePersistentDomain(forName: suite)
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        let settings = BedTermSettings(defaults: defaults)
        // Default — opt-in: off
        XCTAssertFalse(settings.installShellIntegrationOnConnect)
        settings.installShellIntegrationOnConnect = true
        XCTAssertTrue(settings.installShellIntegrationOnConnect)
        // Persists
        let reloaded = BedTermSettings(defaults: defaults)
        XCTAssertTrue(reloaded.installShellIntegrationOnConnect)
    }
}
