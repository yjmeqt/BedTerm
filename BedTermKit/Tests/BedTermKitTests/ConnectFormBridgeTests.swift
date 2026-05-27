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
    private func makeStore() throws -> HostsStore {
        TestKeychain.installInMemory()
        let suite = UUID().uuidString
        let defaults = try #require(UserDefaults(suiteName: suite))
        return HostsStore(
            service: "bt.connectformbridge.test.\(suite)",
            orderKey: "bt.connectformbridge.test.order.\(suite)",
            migrationKey: "bt.connectformbridge.test.migration.\(suite)",
            defaults: defaults
        )
    }

    @Test("prefill returns nil for unknown UUIDs")
    func prefillReturnsNilForUnknownUUID() throws {
        let store = try makeStore()
        let prior = ConnectFormBridge.store
        ConnectFormBridge.store = store
        defer { ConnectFormBridge.store = prior }

        let ptr = UUID().uuidString.withCString { btSwiftConnectFormPrefillJSON($0) }
        #expect(ptr == nil)
    }

    @Test("prefill round-trips a saved password host")
    func prefillRoundTripsSavedPasswordHost() throws {
        let store = try makeStore()
        let prior = ConnectFormBridge.store
        ConnectFormBridge.store = store
        defer { ConnectFormBridge.store = prior }

        let host = SavedHost(
            label: "Mac",
            credential: HostCredential(
                host: "10.0.0.5", port: 22, username: "yi", auth: .password("pw")
            )
        )
        try store.save(host)

        let ptr = try #require(host.id.uuidString.withCString { btSwiftConnectFormPrefillJSON($0) })
        defer { btSwiftConnectFormFreeSnapshot(ptr) }
        let json = String(cString: ptr)
        let dict = try #require(
            try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any]
        )
        #expect(dict["host"] as? String == "10.0.0.5")
        #expect(dict["username"] as? String == "yi")
        #expect(dict["port"] as? String == "22")
        #expect(dict["authIsKey"] as? Bool == false)
        #expect(dict["passwordSet"] as? Bool == true)
    }

    @Test("save persists an add-mode draft and returns id via out_id")
    func saveRoundTripsAddDraft() throws {
        let store = try makeStore()
        let prior = ConnectFormBridge.store
        ConnectFormBridge.store = store
        defer { ConnectFormBridge.store = prior }
        let priorErr = ConnectFormBridge.lastError
        ConnectFormBridge.lastError = nil
        defer { ConnectFormBridge.lastError = priorErr }

        let draft = """
            {"id":"","label":"Mac","host":"10.0.0.5","port":"22","username":"yi","authMode":"password"}
            """
        var outID: UnsafeMutablePointer<CChar>?
        let ok = draft.withCString { draftPtr in
            "secret".withCString { pwPtr in
                btSwiftConnectFormSave(draftPtr, pwPtr, nil, &outID)
            }
        }
        #expect(ok)
        let ptr = try #require(outID)
        defer { btSwiftConnectFormFreeSnapshot(ptr) }
        let idString = String(cString: ptr)
        let uuid = try #require(UUID(uuidString: idString))
        let loaded = try store.load(id: uuid)
        #expect(loaded.credential.host == "10.0.0.5")
        #expect(loaded.credential.port == 22)
        #expect(loaded.credential.username == "yi")
        if case .password(let storedPW) = loaded.credential.auth {
            #expect(storedPW == "secret")
        } else {
            Issue.record("expected .password auth")
        }
    }

    @Test("save returns false + populates lastError on validation failure")
    func saveSurfacesLastError() throws {
        let store = try makeStore()
        let prior = ConnectFormBridge.store
        ConnectFormBridge.store = store
        defer { ConnectFormBridge.store = prior }
        ConnectFormBridge.lastError = nil

        // Empty host → validation failure.
        let draft = """
            {"id":"","label":"Mac","host":"","port":"22","username":"yi","authMode":"password"}
            """
        var outID: UnsafeMutablePointer<CChar>?
        let ok = draft.withCString { draftPtr in
            "secret".withCString { pwPtr in
                btSwiftConnectFormSave(draftPtr, pwPtr, nil, &outID)
            }
        }
        #expect(!ok)
        let errPtr = btSwiftConnectFormLastError()
        defer { btSwiftConnectFormFreeSnapshot(errPtr) }
        #expect(errPtr != nil)
    }

    @Test("free_snapshot is null-safe")
    func freeSnapshotNullSafe() {
        btSwiftConnectFormFreeSnapshot(nil)
    }

    @Test("pick_key callback fires synchronously with nil label when no UI")
    func pickKeyFiresCancel() {
        // Default implementation defers the document picker; the shim
        // should still fire the callback with NULL label so Rust doesn't
        // hang.
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
        #expect(dlsym(handle, "bt_swift_connect_form_prefill_json") != nil)
        #expect(dlsym(handle, "bt_swift_connect_form_free_snapshot") != nil)
        #expect(dlsym(handle, "bt_swift_connect_form_save") != nil)
        #expect(dlsym(handle, "bt_swift_connect_form_last_error") != nil)
        #expect(dlsym(handle, "bt_swift_connect_form_pick_key") != nil)
        #expect(dlsym(handle, "bt_swift_connect_form_take_pending_key_bytes") != nil)
        #expect(dlsym(handle, "bt_swift_connect_form_free_key_bytes") != nil)
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
}
