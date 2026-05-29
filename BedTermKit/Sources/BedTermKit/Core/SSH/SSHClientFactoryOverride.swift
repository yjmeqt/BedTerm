import Foundation

/// Process-wide test hook for substituting the production `SSHClient`
/// implementation with a deterministic stub.
///
/// **DEBUG / UI-test use only.** The `clientFactory` indirection in
/// `ConnectAttempt` was removed when the connect path migrated to
/// `RustTerminalSession`. This type is retained for `MockSSHClient`-based
/// unit tests that construct the client directly.
///
/// For UI tests, use `RustTerminalSessionMockOverride` instead — it
/// installs the mock at the Rust FFI level so the full production
/// `RustTerminalSession` connect path is exercised.
public typealias SSHClientFactory = @MainActor () -> any SSHClient

@MainActor
public enum SSHClientFactoryOverride {
    /// When non-nil, registers a scripted stub in place of the real SSH
    /// transport. Not used by `RustTerminalSession` — see
    /// `RustTerminalSessionMockOverride` for the active UI-test injection
    /// point.
    public static var current: SSHClientFactory?
}

/// Process-wide test hook that installs a named mock script in every new
/// `RustTerminalSession` before its first connect call.
///
/// **DEBUG / UI-test use only.** Set by `UITestSupport` when the app is
/// launched with `-uitest-stubSSH <script>`.
///
/// When `scriptName` is non-nil, `RustTerminalSession.init()` calls
/// `bt_terminal_session_install_mock` with this script name so the session
/// uses scripted mock behaviour instead of real SSH.
@MainActor
public enum RustTerminalSessionMockOverride {
    public static var scriptName: String?
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
