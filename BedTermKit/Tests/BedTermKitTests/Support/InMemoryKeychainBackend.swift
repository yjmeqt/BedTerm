import Foundation

@testable import BedTermKit

/// In-memory `KeychainBackend` used by tests. SPM xctest bundles have no
/// host app and therefore no keychain-access-group entitlement, so the real
/// `SecItem*` calls return errSecMissingEntitlement (-34018). Tests install
/// this backend so the same store logic exercises an in-memory dictionary.
final class InMemoryKeychainBackend: KeychainBackend, @unchecked Sendable {
    private struct Key: Hashable {
        let service: String
        let account: String
    }

    private let lock = NSLock()
    private var storage: [Key: Data] = [:]

    func save(service: String, account: String, data: Data) throws {
        lock.lock()
        defer { lock.unlock() }
        storage[Key(service: service, account: account)] = data
    }

    func load(service: String, account: String) throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        guard let data = storage[Key(service: service, account: account)] else {
            throw KeychainError.notFound
        }
        return data
    }

    func delete(service: String, account: String) {
        lock.lock()
        defer { lock.unlock() }
        storage.removeValue(forKey: Key(service: service, account: account))
    }

    func allAccounts(service: String) -> [String] {
        lock.lock()
        defer { lock.unlock() }
        return storage.keys.filter { $0.service == service }.map(\.account)
    }
}

enum TestKeychain {
    /// Replace the global Keychain backend with a fresh in-memory store.
    /// Call from a suite `init()` so each test instance starts empty.
    static func installInMemory() {
        Keychain.backend = InMemoryKeychainBackend()
    }
}
