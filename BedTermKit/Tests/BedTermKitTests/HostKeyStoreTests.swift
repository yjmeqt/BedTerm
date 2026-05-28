import BedTermIOS
import Foundation
import Testing

@testable import BedTermKit

@Suite("HostKeyStore", .serialized)
struct HostKeyStoreTests {
    init() {
        let suffix = UUID().uuidString
        let service = "com.applovin.yi.bedterm.tests.hostkeys.\(suffix)"
        service.withCString { ptr in
            bt_ios_host_keys_set_test_service(ptr)
        }
    }

    /// FFI-based Verdict mirror matching Rust's host_key_store::Verdict.
    private enum Verdict: Equatable {
        case match
        case mismatch(stored: String, remote: String)
        case unknown
    }

    private func loadFingerprint(host: String, port: Int) -> String? {
        guard let ptr = host.withCString({ bt_ios_host_keys_load($0, UInt16(port)) }) else {
            return nil
        }
        defer { bt_ios_host_keys_free_string(ptr) }
        return String(cString: ptr)
    }

    private func saveFingerprint(_ fp: String, host: String, port: Int) {
        let ok = host.withCString { hostPtr in
            fp.withCString { fpPtr in
                bt_ios_host_keys_save(hostPtr, UInt16(port), fpPtr)
            }
        }
        #expect(ok)
    }

    private func verifyFingerprint(remote: String, host: String, port: Int) -> Verdict {
        var outStored: UnsafeMutablePointer<CChar>?
        let verdict = host.withCString { hostPtr in
            remote.withCString { remotePtr in
                bt_ios_host_keys_verify(hostPtr, UInt16(port), remotePtr, &outStored)
            }
        }
        switch verdict {
        case 0:
            return .match
        case 1:
            let storedStr = outStored.map { String(cString: $0) } ?? ""
            if let ptr = outStored { bt_ios_host_keys_free_string(ptr) }
            return .mismatch(stored: storedStr, remote: remote)
        default:
            return .unknown
        }
    }

    private func removeFingerprint(host: String, port: Int) {
        host.withCString { bt_ios_host_keys_delete($0, UInt16(port)) }
    }

    @Test("first lookup returns nil (no fingerprint stored yet)")
    func firstLookupNil() {
        #expect(loadFingerprint(host: "example.com", port: 22) == nil)
    }

    @Test("storing then looking up returns the same fingerprint")
    func storeThenLookup() {
        saveFingerprint("SHA256:abc", host: "example.com", port: 22)
        #expect(loadFingerprint(host: "example.com", port: 22) == "SHA256:abc")
    }

    @Test("verify returns .match for known fingerprint")
    func verifyMatch() {
        saveFingerprint("SHA256:abc", host: "example.com", port: 22)
        #expect(verifyFingerprint(remote: "SHA256:abc", host: "example.com", port: 22) == .match)
    }

    @Test("verify returns .mismatch for different fingerprint")
    func verifyMismatch() {
        saveFingerprint("SHA256:abc", host: "example.com", port: 22)
        let result = verifyFingerprint(remote: "SHA256:xyz", host: "example.com", port: 22)
        #expect(result == .mismatch(stored: "SHA256:abc", remote: "SHA256:xyz"))
    }

    @Test("verify returns .unknown when nothing stored")
    func verifyUnknown() {
        #expect(verifyFingerprint(remote: "SHA256:abc", host: "example.com", port: 22) == .unknown)
    }

    @Test("remove drops the stored fingerprint")
    func removeDrops() {
        saveFingerprint("SHA256:abc", host: "example.com", port: 22)
        removeFingerprint(host: "example.com", port: 22)
        #expect(loadFingerprint(host: "example.com", port: 22) == nil)
    }
}
