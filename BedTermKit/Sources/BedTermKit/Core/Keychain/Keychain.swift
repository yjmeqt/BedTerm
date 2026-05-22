import Foundation
import Security

enum KeychainError: Error, Equatable {
    case notFound
    case status(OSStatus)
}

protocol KeychainBackend: Sendable {
    func save(service: String, account: String, data: Data) throws
    func load(service: String, account: String) throws -> Data
    func delete(service: String, account: String)
    func allAccounts(service: String) -> [String]
}

enum Keychain {
    // The backend is a static seam so production code calls remain
    // `Keychain.save(...)` while tests can install an in-memory fake.
    // SPM xctest bundles run without a host app and therefore have no
    // keychain-access-group entitlement, so `SecItem*` calls fail with
    // errSecMissingEntitlement (-34018). Tests replace this with
    // `InMemoryKeychainBackend` to exercise the same store logic.
    nonisolated(unsafe) static var backend: any KeychainBackend = SecItemKeychainBackend()

    static func save(service: String, account: String, data: Data) throws {
        try backend.save(service: service, account: account, data: data)
    }

    static func load(service: String, account: String) throws -> Data {
        try backend.load(service: service, account: account)
    }

    static func delete(service: String, account: String) {
        backend.delete(service: service, account: account)
    }

    /// Returns every account name currently stored under the given service.
    /// Used by `HostsStore` to reconcile its UserDefaults index against the Keychain.
    static func allAccounts(service: String) -> [String] {
        backend.allAccounts(service: service)
    }
}

struct SecItemKeychainBackend: KeychainBackend {
    func save(service: String, account: String, data: Data) throws {
        let base: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        SecItemDelete(base as CFDictionary)
        var add = base
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw KeychainError.status(status) }
    }

    func load(service: String, account: String) throws -> Data {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { throw KeychainError.notFound }
        guard status == errSecSuccess, let data = item as? Data else {
            throw KeychainError.status(status)
        }
        return data
    }

    func delete(service: String, account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        SecItemDelete(query as CFDictionary)
    }

    func allAccounts(service: String) -> [String] {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecReturnAttributes as String: true,
            kSecMatchLimit as String: kSecMatchLimitAll
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess, let items = result as? [[String: Any]] else { return [] }
        return items.compactMap { $0[kSecAttrAccount as String] as? String }
    }
}
