import BedTermCoreC
import XCTest

@testable import BedTermKit

@MainActor
final class PersistedSessionSnapshotStoreTests: XCTestCase {
    private func tempDBURL() -> URL {
        FileManager.default.temporaryDirectory
            .appending(path: "bedterm-test-\(UUID().uuidString).sqlite")
    }

    func testListEmptyHost() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let handle = try XCTUnwrap(PersistenceHandle.open(at: url))
        let store = PersistedSessionSnapshotStore(handle: handle)
        let hostID = UUID()
        XCTAssertTrue(store.snapshots(forHost: hostID).isEmpty)
    }

    func testHandleSurvivesAcrossOpens() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }
        // Open once; release; open again.
        do {
            let handle = try XCTUnwrap(PersistenceHandle.open(at: url))
            _ = handle  // touch to silence warnings
        }
        let handle = try XCTUnwrap(PersistenceHandle.open(at: url))
        let store = PersistedSessionSnapshotStore(handle: handle)
        XCTAssertTrue(store.snapshots(forHost: UUID()).isEmpty)
    }
}
