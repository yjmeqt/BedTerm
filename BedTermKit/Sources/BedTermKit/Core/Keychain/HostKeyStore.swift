import Foundation

public struct HostKeyStore {
    public enum Verdict: Equatable {
        case match
        case mismatch(stored: String, remote: String)
        case unknown
    }

    private let service: String

    public init(service: String = "com.applovin.yi.bedterm.hostkeys") {
        self.service = service
    }

    private func account(host: String, port: Int) -> String { "\(host):\(port)" }

    public func fingerprint(host: String, port: Int) throws -> String? {
        do {
            let data = try Keychain.load(service: self.service, account: self.account(host: host, port: port))
            return String(data: data, encoding: .utf8)
        } catch KeychainError.notFound {
            return nil
        }
    }

    public func store(fingerprint: String, host: String, port: Int) throws {
        try Keychain.save(
            service: self.service,
            account: self.account(host: host, port: port),
            data: Data(fingerprint.utf8)
        )
    }

    public func verify(remote: String, host: String, port: Int) throws -> Verdict {
        guard let stored = try fingerprint(host: host, port: port) else { return .unknown }
        if stored == remote { return .match }
        return .mismatch(stored: stored, remote: remote)
    }
}
