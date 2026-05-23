import BedTermCoreC
import Foundation
import Testing

@testable import BedTermKit

@Suite("PersistedSessionSnapshotStore")
@MainActor
struct PersistedSessionSnapshotStoreTests {
    private func tempDBURL() -> URL {
        FileManager.default.temporaryDirectory
            .appending(path: "bedterm-test-\(UUID().uuidString).sqlite")
    }

    @Test
    func listEmptyHost() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let handle = try #require(PersistenceHandle.open(at: url))
        let store = PersistedSessionSnapshotStore(handle: handle)
        let hostID = UUID()
        #expect(store.snapshots(forHost: hostID).isEmpty)
    }

    @Test
    func handleSurvivesAcrossOpens() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }
        do {
            let handle = try #require(PersistenceHandle.open(at: url))
            _ = handle
        }
        let handle = try #require(PersistenceHandle.open(at: url))
        let store = PersistedSessionSnapshotStore(handle: handle)
        #expect(store.snapshots(forHost: UUID()).isEmpty)
    }
}
