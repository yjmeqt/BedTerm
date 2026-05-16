import Foundation
import Testing

@testable import BedTerm

@Suite("CredentialsStore")
struct CredentialsStoreTests {
    private let testService = "com.applovin.yi.bedterm.tests.credentials"

    init() {
        // Clean slate per test instance.
        Keychain.delete(service: self.testService, account: "default")
    }

    @Test("save then load round-trips a password credential")
    func roundTripPassword() throws {
        let store = CredentialsStore(service: testService)
        let cred = HostCredential(
            host: "example.com", port: 22, username: "alice",
            auth: .password("hunter2")
        )
        try store.save(cred)
        let loaded = try store.load()
        #expect(loaded == cred)
    }

    @Test("save then load round-trips a private-key credential with passphrase")
    func roundTripKey() throws {
        let store = CredentialsStore(service: testService)
        let cred = HostCredential(
            host: "10.0.0.5", port: 2222, username: "root",
            auth: .privateKey(Data("KEY-BYTES".utf8), passphrase: "secret")
        )
        try store.save(cred)
        let loaded = try store.load()
        #expect(loaded == cred)
    }

    @Test("load throws notFound when nothing is saved")
    func loadEmpty() {
        let store = CredentialsStore(service: testService)
        #expect(throws: KeychainError.notFound) { try store.load() }
    }

    @Test("delete removes the credential")
    func deleteRemoves() throws {
        let store = CredentialsStore(service: testService)
        try store.save(HostCredential(host: "h", port: 22, username: "u", auth: .password("p")))
        store.delete()
        #expect(throws: KeychainError.notFound) { try store.load() }
    }
}
