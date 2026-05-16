// swiftlint:disable sorted_imports
// swiftformat:disable sortImports blankLineAfterImports
import Foundation
import Testing
@testable import BedTerm
// swiftformat:enable sortImports blankLineAfterImports
// swiftlint:enable sorted_imports

@Suite("HostKeyStore")
struct HostKeyStoreTests {
    private let service = "com.applovin.yi.bedterm.tests.hostkeys"

    init() {
        Keychain.delete(service: self.service, account: "example.com:22")
    }

    @Test("first lookup returns nil (no fingerprint stored yet)")
    func firstLookupNil() throws {
        let store = HostKeyStore(service: service)
        #expect(try store.fingerprint(host: "example.com", port: 22) == nil)
    }

    @Test("storing then looking up returns the same fingerprint")
    func storeThenLookup() throws {
        let store = HostKeyStore(service: service)
        try store.store(fingerprint: "SHA256:abc", host: "example.com", port: 22)
        #expect(try store.fingerprint(host: "example.com", port: 22) == "SHA256:abc")
    }

    @Test("verify returns .match for known fingerprint")
    func verifyMatch() throws {
        let store = HostKeyStore(service: service)
        try store.store(fingerprint: "SHA256:abc", host: "example.com", port: 22)
        #expect(try store.verify(remote: "SHA256:abc", host: "example.com", port: 22) == .match)
    }

    @Test("verify returns .mismatch for different fingerprint")
    func verifyMismatch() throws {
        let store = HostKeyStore(service: service)
        try store.store(fingerprint: "SHA256:abc", host: "example.com", port: 22)
        let result = try store.verify(remote: "SHA256:xyz", host: "example.com", port: 22)
        #expect(result == .mismatch(stored: "SHA256:abc", remote: "SHA256:xyz"))
    }

    @Test("verify returns .unknown when nothing stored")
    func verifyUnknown() throws {
        let store = HostKeyStore(service: service)
        #expect(try store.verify(remote: "SHA256:abc", host: "example.com", port: 22) == .unknown)
    }
}
