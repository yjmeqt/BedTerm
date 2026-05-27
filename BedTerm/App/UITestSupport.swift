import BedTermKit
import Foundation

/// Parses `-uitest-*` launch arguments and wires the corresponding
/// production overrides (`SSHClientFactoryOverride`, `HostsStoreInjection`)
/// before any view mounts. Also exposes accessors the `RootCoordinator`
/// reads when deciding which root VC to install.
///
/// See `BedTermUITests/RustTerminalSmokeUITests.swift` and
/// `BedTermUITests/RustTerminalEndToEndSSHUITests.swift` for the
/// consumers.
enum UITestSupport {
    /// True when `-uitest-skipOnboarding` is passed — the coordinator
    /// will bypass the onboarding screen even if it hasn't been
    /// completed yet.
    static var skipOnboarding: Bool {
        ProcessInfo.processInfo.arguments.contains("-uitest-skipOnboarding")
    }

    /// DEBUG-only direct-mount of the Rust terminal harness for UI tests.
    /// Pass `-uitest-rustTerminalDirect <fixture>` (e.g. `hello`,
    /// `ansiColors`, `blank`) to skip onboarding + Hosts + SSH and land
    /// straight on the Rust VC fed by an in-process byte fixture.
    static var rustTerminalDirectFixture: String? {
        let args = ProcessInfo.processInfo.arguments
        guard let idx = args.firstIndex(of: "-uitest-rustTerminalDirect") else {
            return nil
        }
        let next = idx + 1
        guard next < args.count else { return "hello" }
        let value = args[next]
        // Defensive: ignore the next token if it looks like another flag
        // — `XCUIApplication.launchArguments` flattens the array, so a
        // bare `-uitest-rustTerminalDirect` would otherwise greedily
        // consume an unrelated arg.
        if value.hasPrefix("-") { return "hello" }
        return value
    }

    /// Installs the `-uitest-stubSSH` and `-uitest-injectStubHost`
    /// overrides. Called from `AppDelegate.application(_:didFinishLaunching:)`
    /// so they're in place before any view mounts. Production launches
    /// don't pass these args so this is a no-op outside the UI-test target.
    @MainActor
    static func installOverridesIfNeeded() {
        let args = ProcessInfo.processInfo.arguments
        if let stubIdx = args.firstIndex(of: "-uitest-stubSSH") {
            let next = stubIdx + 1
            let script = (next < args.count && !args[next].hasPrefix("-")) ? args[next] : "hello"
            SSHClientFactoryOverride.current = { makeStubClient(script: script) }
        }
        if args.contains("-uitest-injectStubHost") {
            HostsStoreInjection.current = [stubHost()]
        }
    }

    @MainActor
    private static func makeStubClient(script: String) -> any SSHClient {
        let client = MockSSHClient()
        switch script {
        case "ansiColors":
            let bytes: [UInt8] = Array(
                "\u{1B}[31mred\u{1B}[0m \u{1B}[32mgreen\u{1B}[0m \u{1B}[34mblue\u{1B}[0m\r\n".utf8
            )
            client.script(output: [Data(bytes)])
        case "prompt":
            client.script(output: [Data("bedterm$ ".utf8)])
        case "echo":
            client.script(output: [Data("bedterm$ ".utf8)])
            client.scriptEchoOnWrite()
        default:
            // Default fixture (also used for explicit "hello").
            client.script(output: [Data("Hello, world!\r\n".utf8)])
        }
        return client
    }

    private static func stubHost() -> SavedHost {
        // Stable UUID so the same fake row appears every launch.
        let id = UUID(uuidString: "00000000-0000-0000-0000-00000000B0B0") ?? UUID()
        // NOTE: host must be neither RFC1918 / link-local nor a `.local`
        // mDNS name, so `LocalNetworkPrewarmer.isLAN` returns false and
        // the connect path skips the (real, blocking) NWBrowser prewarm
        // prompt. `example.com` is a non-LAN sentinel — the mock SSH
        // client ignores the host string anyway.
        let credential = HostCredential(
            host: "example.com",
            port: 22,
            username: "test",
            auth: .password("test")
        )
        return SavedHost(id: id, label: "stub", credential: credential)
    }
}
