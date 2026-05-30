import BedTermKit
import Foundation

/// Parses `-uitest-*` launch arguments and wires the corresponding
/// production overrides before any view mounts.
enum UITestSupport {
    // -- MARK: Test injection
    @MainActor
    public enum HostsStoreInjection {
        public static var current: [UITestSupport.EntryRef] = []
    }
    /// True when `-uitest-skipOnboarding` is passed — the coordinator
    /// will bypass the onboarding screen even if it hasn't been
    /// completed yet.
    static var skipOnboarding: Bool {
        ProcessInfo.processInfo.arguments.contains("-uitest-skipOnboarding")
    }

    /// DEBUG-only direct-mount of the Rust terminal harness for UI tests.
    /// Pass `-uitest-rustTerminalDirect <fixture>` (e.g. `hello`)
    /// to skip onboarding + Hosts + SSH and land straight on the Rust VC
    /// fed by an in-process byte fixture.
    static var rustTerminalDirectFixture: String? {
        let args = ProcessInfo.processInfo.arguments
        guard let idx = args.firstIndex(of: "-uitest-rustTerminalDirect") else {
            return nil
        }
        let next = idx + 1
        guard next < args.count else { return "hello" }
        let value = args[next]
        if value.hasPrefix("-") { return "hello" }
        return value
    }

    /// Installs the `-uitest-injectStubHost` override. Called from
    /// `AppDelegate.application(_:didFinishLaunching:)` so it's in place
    /// before any view mounts. Production launches don't pass these args
    /// so this is a no-op outside the UI-test target.
    @MainActor
    static func installOverridesIfNeeded() {
        if args.contains("-uitest-injectStubHost") {
            HostsStoreInjection.current = [stubHost()]
        }
    }

    /// The launch arguments parsed by `UITestSupport`.
    private static var args: [String] {
        ProcessInfo.processInfo.arguments
    }

    private static func stubHost() -> UITestSupport.EntryRef {
        // Stable UUID so the same fake row appears every launch.
        let id = UUID(uuidString: "00000000-0000-0000-0000-00000000B0B0") ?? UUID()
        // NOTE: host must be neither RFC1918 / link-local nor a `.local`
        // mDNS name, so `LocalNetworkPrewarmer.isLAN` returns false and
        // the connect path skips the (real, blocking) NWBrowser prewarm
        // prompt. `example.com` is a non-LAN sentinel — the mock SSH
        // client ignores the host string anyway.
        return EntryRef(
            id: id,
            label: "stub",
            host: "example.com",
            port: 22,
            username: "test"
        )
    }
}

// Inlined from EntryRef.swift (no other callers remain).
extension UITestSupport {
    public struct EntryRef: Identifiable, Sendable {
        public let id: UUID
        public let label: String
        public let host: String
        public let port: Int
        public let username: String
        public init(id: UUID, label: String, host: String, port: Int, username: String) {
            self.id = id; self.label = label; self.host = host
            self.port = port; self.username = username
        }
        public var displayName: String { label.isEmpty ? "\(username)@\(host)" : label }
    }
}
