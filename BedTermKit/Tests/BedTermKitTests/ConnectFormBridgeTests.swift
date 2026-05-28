import BedTermIOS
import Foundation
import Testing

@testable import BedTermKit

/// Exercises the W24c `@_cdecl` shims that the Rust connect-form VC
/// reaches for via `bt_swift_connect_form_*`. The bridge has minimal
/// business logic — these tests pin the symbol names + JSON shape so
/// renaming either side without updating the other breaks loudly.
@Suite("ConnectFormBridge")
@MainActor
struct ConnectFormBridgeTests {
    @Test("pick_key callback fires synchronously with nil label when no UI")
    func pickKeyFiresCancel() {
        final class Flag {
            var fired = false
            var labelWasNil = false
        }
        let flag = Flag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<Flag>.fromOpaque(ctx).release() }
        let callback: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> Void = { ctx, label in
            guard let ctx else { return }
            let pinned = Unmanaged<Flag>.fromOpaque(ctx).takeUnretainedValue()
            pinned.fired = true
            pinned.labelWasNil = (label == nil)
        }
        btSwiftConnectFormPickKey(callback, ctx)
        #expect(flag.fired)
        #expect(flag.labelWasNil)
    }

    @Test("C symbols are reachable via dlsym")
    func cdeclSymbolsReachable() throws {
        let handle = UnsafeMutableRawPointer(bitPattern: -2)
        #expect(dlsym(handle, "bt_swift_connect_form_pick_key") != nil)
        #expect(dlsym(handle, "bt_swift_connect_form_take_pending_key_bytes") != nil)
        #expect(dlsym(handle, "bt_swift_connect_form_free_key_bytes") != nil)
        #expect(dlsym(handle, "bt_swift_hosts_store_save_json") != nil)
    }

    @Test("take_pending_key_bytes round-trips parked bytes and clears storage")
    func takePendingKeyRoundTrips() throws {
        let priorBytes = ConnectFormBridge.pendingKeyBytes
        let priorLabel = ConnectFormBridge.pendingKeyLabel
        defer {
            ConnectFormBridge.pendingKeyBytes = priorBytes
            ConnectFormBridge.pendingKeyLabel = priorLabel
        }

        let payload = Data([0x2D, 0x2D, 0x2D, 0x42, 0x45, 0x47, 0x49, 0x4E])  // "---BEGIN"
        ConnectFormBridge.pendingKeyBytes = payload
        ConnectFormBridge.pendingKeyLabel = "id_ed25519"

        var len: Int = 0
        let ptr = try #require(btSwiftConnectFormTakePendingKeyBytes(&len))
        defer { btSwiftConnectFormFreeKeyBytes(ptr) }
        #expect(len == payload.count)
        let copied = Data(bytes: ptr, count: len)
        #expect(copied == payload)
        // Storage cleared.
        #expect(ConnectFormBridge.pendingKeyBytes == nil)
        #expect(ConnectFormBridge.pendingKeyLabel == nil)

        // Second take returns nil + zero length.
        var len2: Int = 0
        let ptr2 = btSwiftConnectFormTakePendingKeyBytes(&len2)
        #expect(ptr2 == nil)
        #expect(len2 == 0)
    }

    @Test("free_key_bytes is null-safe")
    func freeKeyBytesNullSafe() {
        btSwiftConnectFormFreeKeyBytes(nil)
    }

    @Test("hosts_store_save_json persists a save-outcome through Rust Keychain FFI")
    func hostSavePersistsThroughFFI() throws {
        let suffix = UUID().uuidString
        let svc = "bt.hostssave.test.\(suffix)"
        let ord = "bt.hostssave.test.order.\(suffix)"
        svc.withCString { sPtr in
            ord.withCString { oPtr in
                bt_ios_hosts_set_test_service(sPtr, oPtr)
            }
        }
        defer { bt_ios_hosts_set_test_service(nil, nil) }

        // Build a SaveOutcome JSON as Rust would produce it.
        let id = UUID().uuidString
        let json =
            "{\"id\":\"\(id)\",\"label\":\"Saved Host\",\"host\":\"10.0.0.5\","
            + "\"port\":2222,\"username\":\"yi\",\"authIsKey\":false,\"password\":\"hunter2\"}"
        json.withCString { btSwiftHostsStoreSaveJson($0) }

        // Verify via direct FFI.
        guard let snapshotPtr = bt_ios_hosts_snapshot_json() else {
            Issue.record("snapshot_json returned NULL")
            return
        }
        defer { bt_ios_hosts_free_string(snapshotPtr) }
        let snapshot = String(cString: snapshotPtr)
        guard let data = snapshot.data(using: .utf8),
            let items = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else {
            Issue.record("failed to parse snapshot JSON")
            return
        }
        #expect(items.count == 1)
        let item = try #require(items.first)
        #expect(item["label"] as? String == "Saved Host")
        #expect(item["host"] as? String == "10.0.0.5")
        #expect(item["port"] as? Int == 2222)
        #expect(item["username"] as? String == "yi")
    }

    @Test("hosts_store_save_json is null-safe")
    func hostSaveNullSafe() {
        btSwiftHostsStoreSaveJson(nil)
    }
}
