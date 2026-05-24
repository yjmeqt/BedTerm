import BedTermCoreC
import Foundation
import Testing

@testable import BedTermKit

@Suite("TerminalSession persistence wiring")
@MainActor
struct TerminalSessionPersistenceTests {
    private func tempDBURL() -> URL {
        FileManager.default.temporaryDirectory
            .appending(path: "bedterm-test-\(UUID().uuidString).sqlite")
    }

    @Test
    func snapshotRowExistsAfterAttach() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let handle = try #require(PersistenceHandle.open(at: url))
        let store = PersistedSessionSnapshotStore(handle: handle)
        let hostID = UUID()

        let session = TerminalSession(
            client: MockSSHClient(),
            hostID: hostID,
            persistence: handle
        )

        store.reload(forHost: hostID)
        #expect(store.snapshots(forHost: hostID).count == 1)

        session.disconnect()

        store.reload(forHost: hostID)
        let updated = store.snapshots(forHost: hostID).first
        #expect(updated?.killReason == .userKilled)
    }

    @Test
    func networkDropClassifiedCorrectly() async throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let handle = try #require(PersistenceHandle.open(at: url))
        let store = PersistedSessionSnapshotStore(handle: handle)
        let hostID = UUID()

        let client = MockSSHClient()
        client.scriptConnectError(.peerReset)

        let session = TerminalSession(
            client: client,
            hostID: hostID,
            persistence: handle
        )

        await session.connect(
            credential: HostCredential(
                host: "localhost", port: 22, username: "test",
                auth: .password("test")
            ),
            initialPTY: .init(cols: 80, rows: 24)
        )

        store.reload(forHost: hostID)
        let snap = store.snapshots(forHost: hostID).first
        #expect(snap?.killReason == .networkDrop)
    }
}
