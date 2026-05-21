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

    func testScriptEmitsOsc133Sequences() throws {
        let body = try XCTUnwrap(ShellIntegrationScript.load())
        // The three OSC 133 boundaries we care about should all appear in the
        // script as escape literals so the remote shell will emit them.
        XCTAssertTrue(body.contains(#"\033]133;%s"#))
        // The zsh + bash hook entry points.
        XCTAssertTrue(body.contains("__bedterm_precmd"))
        XCTAssertTrue(body.contains("__bedterm_preexec"))
        // Idempotence guard so re-source doesn't double-install hooks.
        XCTAssertTrue(body.contains("__BEDTERM_INTEGRATION_INSTALLED"))
    }

    // MARK: - Bootstrap payload

    func testBootstrapPayloadWrapsScriptInHeredoc() throws {
        let payload = try XCTUnwrap(ShellIntegrationScript.bootstrapPayload())
        // Leading space + HISTCONTROL=ignorespace keeps the bootstrap out of
        // the user's shell history.
        XCTAssertTrue(payload.hasPrefix(" HISTCONTROL=ignorespace"))
        // Heredoc framing: sentinel appears twice (open + close) and the
        // payload terminates with a newline so the remote shell executes it.
        let parts = payload.components(
            separatedBy: ShellIntegrationScript.heredocSentinel)
        let sentinelCount = parts.count - 1
        XCTAssertEqual(sentinelCount, 2)
        XCTAssertTrue(payload.hasSuffix("\n"))
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
