import Foundation
import Observation

@MainActor
@Observable
final class ConnectionViewModel {
    enum AuthChoice: Equatable { case password, privateKey }

    struct PendingMismatch: Equatable {
        let stored: String
        let remote: String
        let host: String
        let port: Int
    }

    var host: String = ""
    var port: String = "22"
    var username: String = ""
    var password: String = ""
    var privateKey: Data?
    var passphrase: String = ""
    var auth: AuthChoice = .password

    var errorMessage: String?
    var isConnecting = false
    var isPrewarming = false
    var permissionDenied = false
    private(set) var pendingMismatch: PendingMismatch?

    private let credentialsStore: CredentialsStore
    private let clientFactory: () -> any SSHClient
    private(set) var lastSession: TerminalSession?

    init(
        credentialsStore: CredentialsStore = CredentialsStore(),
        clientFactory: @escaping () -> any SSHClient
    ) {
        self.credentialsStore = credentialsStore
        self.clientFactory = clientFactory
    }

    func loadSaved() {
        if let saved = try? credentialsStore.load() {
            host = saved.host
            port = String(saved.port)
            username = saved.username
            switch saved.auth {
            case .password(let pw):
                auth = .password
                password = pw
            case .privateKey(let key, let pass):
                auth = .privateKey
                privateKey = key
                passphrase = pass ?? ""
            }
        }
    }

    func buildCredential() -> HostCredential? {
        guard let portValue = Int(port), portValue > 0, !host.isEmpty, !username.isEmpty else {
            errorMessage = "Host, port and username are required."
            return nil
        }
        let authMethod: HostCredential.AuthMethod
        switch auth {
        case .password:
            authMethod = .password(password)
        case .privateKey:
            guard let key = privateKey else {
                errorMessage = "Please import a private key file."
                return nil
            }
            authMethod = .privateKey(key, passphrase: passphrase.isEmpty ? nil : passphrase)
        }
        return HostCredential(host: host, port: portValue, username: username, auth: authMethod)
    }

    /// Returns a connected session, or sets `errorMessage` / `pendingMismatch` on failure.
    func connect() async -> TerminalSession? {
        guard let credential = buildCredential() else { return nil }
        errorMessage = nil
        pendingMismatch = nil
        permissionDenied = false
        isConnecting = true
        defer { isConnecting = false }

        // R7: trigger iOS local-network permission prompt before SSH for LAN
        // targets so the first connect does not race the prompt and fail.
        let needsPrewarm =
            LocalNetworkPrewarmer.isLAN(host: credential.host)
            && !LocalNetworkPrewarmer.shared.hasGrantedBefore
        if needsPrewarm {
            isPrewarming = true
            let outcome = await LocalNetworkPrewarmer.shared.ensurePermission(forHost: credential.host)
            isPrewarming = false
            if outcome == .denied {
                permissionDenied = true
                errorMessage =
                    "Local network access is required to reach \(credential.host). Open Settings to enable it."
                return nil
            }
            // .unknown (timed out without a clear signal) — fall through and let the
            // SSH attempt happen; if it really is denied, the SSH error will reflect it.
        }

        let session = TerminalSession(client: clientFactory())
        await session.connect(credential: credential, initialPTY: .init(cols: 80, rows: 24))

        switch session.state {
        case .open:
            try? credentialsStore.save(credential)
            lastSession = session
            return session
        case .closed(reason: let reason):
            // Surface host-key mismatch as a structured prompt; everything else is a flat error.
            if reason.hasPrefix("Host key changed.") {
                let parsed = Self.parseMismatch(reason)
                pendingMismatch = PendingMismatch(
                    stored: parsed.stored,
                    remote: parsed.remote,
                    host: credential.host,
                    port: credential.port
                )
            } else {
                errorMessage = reason
            }
            return nil
        default:
            errorMessage = "Unknown error."
            return nil
        }
    }

    private static func parseMismatch(_ reason: String) -> (stored: String, remote: String) {
        let lines = reason.components(separatedBy: "\n")
        let stored = lines.first(where: { $0.hasPrefix("Stored: ") })?.dropFirst("Stored: ".count) ?? ""
        let remote = lines.first(where: { $0.hasPrefix("Remote: ") })?.dropFirst("Remote: ".count) ?? ""
        return (String(stored), String(remote))
    }
}
