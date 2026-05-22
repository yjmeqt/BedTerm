import XCTest

@testable import BedTermKit

@MainActor
final class SessionSnapshotTests: XCTestCase {
    // MARK: - SessionSnapshotStore

    func testRecordAddsSnapshotForHost() {
        let store = SessionSnapshotStore()
        let hostID = UUID()
        let snap = makeSnapshot(hostID: hostID, command: "ls")
        store.record(snap)
        XCTAssertEqual(store.snapshots.count, 1)
        XCTAssertEqual(store.snapshots(forHost: hostID).count, 1)
        XCTAssertEqual(store.snapshot(id: snap.id)?.lastCommand, "ls")
    }

    func testRecordEnforcesPerHostCap() {
        let store = SessionSnapshotStore()
        let hostA = UUID()
        let hostB = UUID()
        // Fill host A past the cap and host B with a couple of entries.
        for i in 0..<(SessionSnapshotStore.perHostCap + 3) {
            store.record(makeSnapshot(hostID: hostA, command: "a-\(i)"))
        }
        store.record(makeSnapshot(hostID: hostB, command: "b-0"))
        store.record(makeSnapshot(hostID: hostB, command: "b-1"))

        // Host A should be capped at perHostCap; host B untouched.
        XCTAssertEqual(store.snapshots(forHost: hostA).count, SessionSnapshotStore.perHostCap)
        XCTAssertEqual(store.snapshots(forHost: hostB).count, 2)

        // Oldest host-A snapshot (`a-0`) must be gone; newest (`a-12`) present.
        let aCommands = store.snapshots(forHost: hostA).compactMap { $0.lastCommand }
        XCTAssertFalse(aCommands.contains("a-0"))
        XCTAssertTrue(aCommands.contains("a-\(SessionSnapshotStore.perHostCap + 2)"))
    }

    func testDiscardRemovesSpecificSnapshot() {
        let store = SessionSnapshotStore()
        let hostID = UUID()
        let snap = makeSnapshot(hostID: hostID, command: "rm")
        store.record(snap)
        store.discard(id: snap.id)
        XCTAssertTrue(store.snapshots.isEmpty)
    }

    // MARK: - Resume command quoting

    func testBuildResumeCommandQuotesPath() {
        let cmd = KilledSessionDetailScreen.buildResumeCommand(cwd: "/home/yj")
        XCTAssertEqual(cmd, "cd '/home/yj'\n")
    }

    func testBuildResumeCommandEscapesSingleQuotes() {
        let cmd = KilledSessionDetailScreen.buildResumeCommand(cwd: "/tmp/it's mine")
        XCTAssertEqual(cmd, "cd '/tmp/it'\\''s mine'\n")
    }

    // MARK: - Path abbreviation

    func testAbbreviateLinuxHome() {
        XCTAssertEqual(SessionSnapshotPathAbbreviator.abbreviate("/home/yj/code"), "~/code")
        XCTAssertEqual(SessionSnapshotPathAbbreviator.abbreviate("/home/yj"), "~")
    }

    func testAbbreviateMacHome() {
        XCTAssertEqual(SessionSnapshotPathAbbreviator.abbreviate("/Users/yj/code"), "~/code")
    }

    func testAbbreviateRoot() {
        XCTAssertEqual(SessionSnapshotPathAbbreviator.abbreviate("/root/log"), "~/log")
        XCTAssertEqual(SessionSnapshotPathAbbreviator.abbreviate("/root"), "~")
    }

    func testAbbreviatePassesThroughUnknown() {
        XCTAssertEqual(SessionSnapshotPathAbbreviator.abbreviate("/var/log"), "/var/log")
    }

    // MARK: - Kill reason copy

    func testKillReasonLocalizedReasonNonEmpty() {
        let reasons: [SessionSnapshot.KillReason] = [
            .userKilled, .remoteLogout, .networkDrop, .appRelaunch, .swapEvicted
        ]
        for reason in reasons {
            XCTAssertFalse(reason.localizedReason.isEmpty, "reason \(reason) should have copy")
        }
    }

    // MARK: - Helpers

    private func makeSnapshot(hostID: UUID, command: String) -> SessionSnapshot {
        SessionSnapshot(
            hostID: hostID,
            blocks: [],
            lastCwd: "/home/yj",
            lastCommand: command,
            lastExitCode: 0,
            killReason: .userKilled
        )
    }
}
