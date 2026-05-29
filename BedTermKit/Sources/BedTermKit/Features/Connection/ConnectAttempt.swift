import Foundation

/// Runs a single SSH connection attempt: LAN prewarm if needed, then
/// `RustTerminalSession.connect`.
///
/// One driver, two callers — both the saved-hosts row tap and the legacy
/// form route go through this so the connect path stays in lockstep.
@MainActor
final class ConnectAttempt {
    enum Outcome {
        case session(RustTerminalSession)
        case mismatch(stored: String, remote: String, host: String, port: Int)
        case error(message: String, permissionDenied: Bool)
    }

    private let prewarmer: LocalNetworkPrewarmer

    init(prewarmer: LocalNetworkPrewarmer? = nil) {
        self.prewarmer = prewarmer ?? .shared
    }

    /// Run the attempt. `onPrewarm(true)` fires when the LAN permission
    /// prompt is in flight; `onPrewarm(false)` fires when it resolves.
    /// `bootstrapPayload` is the shell-integration heredoc to push after
    /// the remote shell emits its first byte; pass `nil` to keep the
    /// channel pristine (default).
    func run(
        credential: HostCredential,
        bootstrapPayload: String? = nil,
        onPrewarm: (@MainActor (Bool) -> Void)? = nil
    ) async -> Outcome {
        let needsPrewarm =
            LocalNetworkPrewarmer.isLAN(host: credential.host)
            && !self.prewarmer.hasGrantedBefore
        if needsPrewarm {
            onPrewarm?(true)
            let outcome = await self.prewarmer.ensurePermission(forHost: credential.host)
            onPrewarm?(false)
            if outcome == .denied {
                let msg = String(
                    localized:
                        "Local network access is required to reach \(credential.host). Open Settings to enable it."
                )
                return .error(message: msg, permissionDenied: true)
            }
            // .unknown — fall through and let SSH surface the real failure.
        }

        let session = RustTerminalSession()
        await session.connect(
            credential: credential,
            initialPTY: .init(cols: 80, rows: 24),
            bootstrapPayload: bootstrapPayload
        )

        switch session.state {
        case .open:
            return .session(session)
        case .closed(reason: let reason):
            if case .hostKeyMismatch(let stored, let remote) = session.lastError {
                return .mismatch(
                    stored: stored, remote: remote,
                    host: credential.host, port: credential.port
                )
            }
            return .error(message: reason, permissionDenied: false)
        default:
            return .error(message: String(localized: "Unknown error."), permissionDenied: false)
        }
    }
}
