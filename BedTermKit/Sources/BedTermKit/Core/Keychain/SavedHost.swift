import Foundation

/// SSH credential payload — host endpoint plus the auth secret. Persisted
/// as the inner field of `SavedHost`; never stored on its own anymore.
public struct HostCredential: Equatable, Codable, Sendable {
    public enum AuthMethod: Equatable, Codable, Sendable {
        case password(String)
        case privateKey(Data, passphrase: String?)
    }

    public var host: String
    public var port: Int
    public var username: String
    public var auth: AuthMethod

    public init(host: String, port: Int, username: String, auth: AuthMethod) {
        self.host = host
        self.port = port
        self.username = username
        self.auth = auth
    }
}

/// One row in the user's saved-hosts list.
///
/// Identity is the `id` UUID — never the host/user tuple — so the user can save
/// duplicate `(host, port, username)` rows (with different labels or auth methods)
/// without the list collapsing them.
public struct SavedHost: Codable, Equatable, Identifiable {
    public let id: UUID
    public var label: String
    public var credential: HostCredential

    public init(id: UUID = UUID(), label: String, credential: HostCredential) {
        self.id = id
        self.label = label
        self.credential = credential
    }
}
