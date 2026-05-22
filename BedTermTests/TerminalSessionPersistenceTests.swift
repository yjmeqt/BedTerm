import BedTermCoreC
import XCTest

@testable import BedTermKit

@MainActor
final class TerminalSessionPersistenceTests: XCTestCase {
    private func tempDBURL() -> URL {
        FileManager.default.temporaryDirectory
            .appending(path: "bedterm-test-\(UUID().uuidString).sqlite")
    }

    /// Smoke test: creating a `TerminalSession` with a `PersistenceHandle`
    /// inserts a row into `snapshots`, and calling `disconnect()` writes the
    /// kill metadata (kill_reason = 0 / userKilled).
    func testSnapshotRowExistsAfterAttach() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }

        let handle = try XCTUnwrap(PersistenceHandle.open(at: url))
        let store = PersistedSessionSnapshotStore(handle: handle)
        let hostID = UUID()

        let session = TerminalSession(
            client: MockSSHClient(),
            hostID: hostID,
            persistence: handle
        )

        // The attach call inside init should have inserted the snapshot row.
        XCTAssertEqual(store.snapshots(forHost: hostID).count, 1)

        // Disconnect → recordKill(reason: .userKilled) runs → kill_reason is set.
        session.disconnect()

        // After disconnect the snapshot's killReason should be set.
        let updated = store.snapshots(forHost: hostID).first
        XCTAssertEqual(updated?.killReason, .userKilled)
    }

    /// Verify that network-drop errors are classified as `.networkDrop` and
    /// the snapshot row reflects that.
    func testNetworkDropClassifiedCorrectly() async throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }

        let handle = try XCTUnwrap(PersistenceHandle.open(at: url))
        let store = PersistedSessionSnapshotStore(handle: handle)
        let hostID = UUID()

        let client = MockSSHClient()
        client.scriptConnectError(.peerReset)

        let session = TerminalSession(
            client: client,
            hostID: hostID,
            persistence: handle
        )

        // connect() surfaces the scripted error → recordKill(.networkDrop).
        await session.connect(credential: .mock, initialPTY: .init(cols: 80, rows: 24))

        let snap = store.snapshots(forHost: hostID).first
        XCTAssertEqual(snap?.killReason, .networkDrop)
    }
}

// MARK: - Test helpers

extension HostCredential {
    static var mock: HostCredential {
        HostCredential(
            host: "localhost",
            port: 22,
            username: "test",
            auth: .password("test")
        )
    }
}
