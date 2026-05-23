import Foundation
import Testing

@testable import BedTermKit

@Suite("PersistedSessionSnapshotStore.attach")
@MainActor
struct PersistedSessionSnapshotStoreAttachTests {
    private func tempDBURL() -> URL {
        FileManager.default.temporaryDirectory
            .appending(path: "bedterm-attach-test-\(UUID().uuidString).sqlite")
    }

    @Test
    func reloadIsNoOpWhenHandleNil() {
        let store = PersistedSessionSnapshotStore(handle: nil)
        store.reload(forHost: UUID())
        #expect(store.snapshots.isEmpty)
        #expect(store.snapshots(forHost: UUID()).isEmpty)
    }

    @Test
    func attachUpgradesNoOpStoreInPlace() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }

        let store = PersistedSessionSnapshotStore(handle: nil)
        let identityBefore = ObjectIdentifier(store)

        let handle = try #require(PersistenceHandle.open(at: url))
        store.attach(handle: handle)

        #expect(ObjectIdentifier(store) == identityBefore)
        // A real reload against an empty SQLite DB should succeed (no rows).
        store.reload(forHost: UUID())
        #expect(store.snapshots.isEmpty)
        #expect(store.snapshots(forHost: UUID()).isEmpty)
    }

    @Test
    func attachIsIdempotentForSameHandle() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }

        let store = PersistedSessionSnapshotStore(handle: nil)
        let handle = try #require(PersistenceHandle.open(at: url))
        store.attach(handle: handle)
        store.attach(handle: handle)
        #expect(store.snapshots.isEmpty)
    }
}
