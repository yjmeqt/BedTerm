import Foundation

/// Process-wide test hook for substituting the production `SSHClient`
/// implementation with a deterministic stub.
///
/// **DEBUG / UI-test use only.** Production launches never set
/// `current`, so `ConnectAttempt` falls through to `CitadelSSHClient()`
/// exactly as before. The override exists so end-to-end UI tests can
/// route every saved-host Connect tap through a scripted `MockSSHClient`
/// without touching the network — the full
/// `IosTerminalView` → `TerminalSession` → `SSHClient` → Rust glyph
/// renderer stack stays intact, only the wire layer is mocked.
///
/// See `BedTermUITests/RustTerminalEndToEndSSHUITests.swift` for the
/// consumer, and `BedTermApp` for the launch-arg parsing that sets
/// `current` at startup.
public typealias SSHClientFactory = @MainActor () -> any SSHClient

@MainActor
public enum SSHClientFactoryOverride {
    /// When non-nil, `ConnectAttempt`'s default client factory delegates
    /// to this closure instead of constructing `CitadelSSHClient()`.
    public static var current: SSHClientFactory?
}

/// Process-wide test hook for pre-populating the saved-hosts list at
/// `HostsViewModel.load()` time without touching the Keychain.
///
/// **DEBUG / UI-test use only.** When `current` is non-nil and
/// non-empty, those entries are prepended to the Keychain-backed list
/// so the test can tap a known row without driving the add-host form.
/// Production launches never set `current`.
@MainActor
public enum HostsStoreInjection {
    public static var current: [SavedHost] = []
}
