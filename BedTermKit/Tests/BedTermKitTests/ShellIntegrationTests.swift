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
        // The SSH bootstrap writes the bootstrap
        // payload verbatim to ~/.cache/bedterm/integration.sh — no heredoc
        // wrapping, no HISTCONTROL prefix. So bootstrapPayload must equal
        // the raw script body returned by load().
        let body = try #require(ShellIntegrationScript.load())
        let payload = try #require(ShellIntegrationScript.bootstrapPayload())
        #expect(payload == body)
    }
}
