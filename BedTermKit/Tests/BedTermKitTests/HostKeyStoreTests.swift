import BedTermCoreC
import Foundation
import Testing

@testable import BedTermKit

@Suite("HostKeyStore", .serialized)
struct HostKeyStoreTests {
    init() {
        // Route Rust-side reads/writes to a fresh per-test in-memory
        // backend. SPM xctest bundles lack the keychain-access-group
        // entitlement, so the real `SecItem*` path would fail with
        // errSecMissingEntitlement (-34018).
        let suffix = UUID().uuidString
        let service = "com.applovin.yi.bedterm.tests.hostkeys.\(suffix)"
        service.withCString { ptr in
            bt_ios_host_keys_set_test_service(ptr)
        }
    }

    @Test("first lookup returns nil (no fingerprint stored yet)")
    func firstLookupNil() throws {
        let store = HostKeyStore()
        #expect(try store.fingerprint(host: "example.com", port: 22) == nil)
    }

    @Test("storing then looking up returns the same fingerprint")
    func storeThenLookup() throws {
        let store = HostKeyStore()
        try store.store(fingerprint: "SHA256:abc", host: "example.com", port: 22)
        #expect(try store.fingerprint(host: "example.com", port: 22) == "SHA256:abc")
    }

    @Test("verify returns .match for known fingerprint")
    func verifyMatch() throws {
        let store = HostKeyStore()
        try store.store(fingerprint: "SHA256:abc", host: "example.com", port: 22)
        #expect(try store.verify(remote: "SHA256:abc", host: "example.com", port: 22) == .match)
    }

    @Test("verify returns .mismatch for different fingerprint")
    func verifyMismatch() throws {
        let store = HostKeyStore()
        try store.store(fingerprint: "SHA256:abc", host: "example.com", port: 22)
        let result = try store.verify(remote: "SHA256:xyz", host: "example.com", port: 22)
        #expect(result == .mismatch(stored: "SHA256:abc", remote: "SHA256:xyz"))
    }

    @Test("verify returns .unknown when nothing stored")
    func verifyUnknown() throws {
        let store = HostKeyStore()
        #expect(try store.verify(remote: "SHA256:abc", host: "example.com", port: 22) == .unknown)
    }

    @Test("remove drops the stored fingerprint")
    func removeDrops() throws {
        let store = HostKeyStore()
        try store.store(fingerprint: "SHA256:abc", host: "example.com", port: 22)
        store.remove(host: "example.com", port: 22)
        #expect(try store.fingerprint(host: "example.com", port: 22) == nil)
    }
}
