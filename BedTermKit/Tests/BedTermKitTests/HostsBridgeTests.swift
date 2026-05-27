import BedTermCoreC
import Foundation
import Testing

@testable import BedTermKit

/// Exercises the W24b `@_cdecl` shims that the Rust hosts list VC reaches
/// for via `bt_swift_hosts_*`. The bridge has minimal business logic —
/// these tests pin the symbol names + JSON shape so renaming either side
/// without updating the other breaks the suite loudly.
@Suite("HostsBridge")
@MainActor
struct HostsBridgeTests {
    @Test("snapshot JSON returns an empty array for an empty store")
    func snapshotEmpty() throws {
        installTestService()
        defer { clearTestService() }
        let prior = HostsBridge.store
        HostsBridge.store = HostsStore()
        defer { HostsBridge.store = prior }

        let json = HostsBridge.snapshotJSON()
        let parsed =
            try JSONSerialization.jsonObject(
                with: Data(json.utf8), options: []) as? [Any]
        #expect(parsed?.isEmpty == true)
    }

    @Test("connect handler receives the parsed UUID")
    func connectHandlerReceivesUUID() throws {
        let prior = HostsBridge.connectHandler
        defer { HostsBridge.connectHandler = prior }

        var received: UUID?
        HostsBridge.connectHandler = { id in received = id }

        let uuid = UUID()
        uuid.uuidString.withCString { ptr in
            btSwiftHostsConnect(ptr)
        }
        #expect(received == uuid)
    }

    @Test("connect handler ignores invalid UUIDs")
    func connectHandlerIgnoresInvalidUUID() throws {
        let prior = HostsBridge.connectHandler
        defer { HostsBridge.connectHandler = prior }

        var fired = false
        HostsBridge.connectHandler = { _ in fired = true }
        "not-a-uuid".withCString { ptr in
            btSwiftHostsConnect(ptr)
        }
        #expect(!fired)
    }

    @Test("delete shim returns false for invalid UUIDs")
    func deleteShimRejectsBadInput() throws {
        let ok = "garbage".withCString { btSwiftHostsDelete($0) }
        #expect(!ok)
    }

    @Test("free_snapshot is null-safe")
    func freeSnapshotNullSafe() {
        btSwiftHostsFreeSnapshot(nil)  // must not crash.
    }

    @Test("snapshot pointer round-trips through strdup/free")
    func snapshotPointerRoundTrip() throws {
        installTestService()
        defer { clearTestService() }
        let prior = HostsBridge.store
        HostsBridge.store = HostsStore()
        defer { HostsBridge.store = prior }

        let ptr = btSwiftHostsSnapshotJSON()
        #expect(ptr != nil)
        if let ptr {
            let json = String(cString: ptr)
            // Must be valid JSON.
            #expect(((try? JSONSerialization.jsonObject(with: Data(json.utf8))) != nil))
            btSwiftHostsFreeSnapshot(ptr)
        }
    }

    private func installTestService() {
        let suffix = UUID().uuidString
        let svc = "bt.hostsbridge.test.\(suffix)"
        let ord = "bt.hostsbridge.test.order.\(suffix)"
        svc.withCString { sPtr in
            ord.withCString { oPtr in
                bt_ios_hosts_set_test_service(sPtr, oPtr)
            }
        }
    }

    private func clearTestService() {
        bt_ios_hosts_set_test_service(nil, nil)
    }

    @Test("C symbols are reachable via dlsym")
    func cdeclSymbolsReachable() throws {
        let handle = UnsafeMutableRawPointer(bitPattern: -2)
        #expect(dlsym(handle, "bt_swift_hosts_snapshot_json") != nil)
        #expect(dlsym(handle, "bt_swift_hosts_free_snapshot") != nil)
        #expect(dlsym(handle, "bt_swift_hosts_delete") != nil)
        #expect(dlsym(handle, "bt_swift_hosts_connect") != nil)
    }
}
