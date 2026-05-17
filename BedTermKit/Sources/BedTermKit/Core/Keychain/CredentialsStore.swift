import Foundation

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

public struct CredentialsStore {
    private let service: String
    private let account = "default"

    public init(service: String = "com.applovin.yi.bedterm.credentials") {
        self.service = service
    }

    public func save(_ credential: HostCredential) throws {
        let data = try JSONEncoder().encode(credential)
        try Keychain.save(service: self.service, account: self.account, data: data)
    }

    public func load() throws -> HostCredential {
        let data = try Keychain.load(service: self.service, account: self.account)
        return try JSONDecoder().decode(HostCredential.self, from: data)
    }

    public func delete() {
        Keychain.delete(service: self.service, account: self.account)
    }
}
