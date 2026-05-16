import Foundation
import Security

enum KeychainError: Error, Equatable {
    case notFound
    case status(OSStatus)
}

enum Keychain {
    static func save(service: String, account: String, data: Data) throws {
        // swiftformat:disable trailingCommas
        let base: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        // swiftformat:enable trailingCommas
        SecItemDelete(base as CFDictionary)
        var add = base
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw KeychainError.status(status) }
    }

    static func load(service: String, account: String) throws -> Data {
        // swiftformat:disable trailingCommas
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        // swiftformat:enable trailingCommas
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { throw KeychainError.notFound }
        guard status == errSecSuccess, let data = item as? Data else {
            throw KeychainError.status(status)
        }
        return data
    }

    static func delete(service: String, account: String) {
        // swiftformat:disable trailingCommas
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        // swiftformat:enable trailingCommas
        SecItemDelete(query as CFDictionary)
    }
}
