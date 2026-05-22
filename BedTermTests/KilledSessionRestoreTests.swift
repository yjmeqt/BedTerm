import XCTest

@testable import BedTermKit

@MainActor
final class KilledSessionRestoreTests: XCTestCase {
    private func tempDBURL() -> URL {
        FileManager.default.temporaryDirectory
            .appending(path: "bedterm-restore-test-\(UUID().uuidString).sqlite")
    }

    // MARK: - Nil-for-missing-snapshot

    /// `openReplay` must return `nil` for a snapshot that was never inserted.
    func testOpenReplayReturnsNilForUnknownSnapshot() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }

        let handle = try XCTUnwrap(PersistenceHandle.open(at: url))
        let result = handle.openReplay(snapshotID: UUID())
        XCTAssertNil(result, "openReplay must return nil for a never-inserted snapshot")
    }

    // MARK: - Round-trip: attach → feed DCS block → kill → openReplay

    /// Feeds one complete OSC-133-style DCS block through a `TerminalCore`
    /// that has `PersistenceHandle.attach` called on it, then calls
    /// `openReplay` and asserts a non-nil `TerminalCore` comes back.
    func testOpenReplayReturnsTerminalCoreAfterBlockCapture() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }

        let handle = try XCTUnwrap(PersistenceHandle.open(at: url))
        let snapshotID = UUID()
        let hostID = UUID()

        // Create and attach a TerminalCore directly — bypass TerminalSession
        // so we don't need a real SSH client.
        let core = TerminalCore(cols: 80, rows: 24)
        handle.attach(terminal: core, snapshotID: snapshotID, hostID: hostID)

        // Feed one complete DCS block so the persistence sink captures it.
        // Format: ESC P $ d <hex-encoded-JSON> 0x9C
        core.feed(dcsBlock(pwd: "/tmp", command: "echo hi", output: b("hi\n"), exit: 0))
        // A second Precmd seals the first block in the DB.
        core.feed(dcsFrame(json: #"{"hook":"Precmd","value":{"pwd":"/tmp"}}"#))

        // Record the kill so the snapshot row is complete.
        handle.recordKill(
            snapshotID: snapshotID,
            reason: .userKilled,
            lastCwd: "/tmp",
            lastCommand: "echo hi",
            lastExitCode: 0
        )

        // Now replay — should return a non-nil TerminalCore with blocks.
        let replayCore = handle.openReplay(snapshotID: snapshotID)
        XCTAssertNotNil(replayCore, "openReplay must return a TerminalCore for a snapshot with captured blocks")
        if let replayCore {
            XCTAssertEqual(replayCore.blockCount, 1, "Replayed TerminalCore must have exactly one block")
        }
    }
}

// MARK: - DCS helpers

/// Build a hex-encoded DCS frame: `ESC P $ d <hex(json)> ST` (ST = 0x9C).
private func dcsFrame(json: String) -> Data {
    var bytes: [UInt8] = [0x1B, UInt8(ascii: "P"), UInt8(ascii: "$"), UInt8(ascii: "d")]
    for byte in json.utf8 {
        let hi = (byte >> 4) & 0x0F
        let lo = byte & 0x0F
        bytes.append(hi < 10 ? hi + 48 : hi + 87)  // hex digit
        bytes.append(lo < 10 ? lo + 48 : lo + 87)
    }
    bytes.append(0x9C)  // ST (String Terminator)
    return Data(bytes)
}

/// Build Precmd → Preexec → output → CommandFinished block.
private func dcsBlock(pwd: String, command: String, output: Data, exit: Int) -> Data {
    var data = Data()
    data.append(dcsFrame(json: #"{"hook":"Precmd","value":{"pwd":"\#(pwd)"}}"#))
    data.append(dcsFrame(json: #"{"hook":"Preexec","value":{"command":"\#(command)"}}"#))
    data.append(output)
    data.append(dcsFrame(json: #"{"hook":"CommandFinished","value":{"exit_code":\#(exit)}}"#))
    return data
}

/// Convenience to turn a string literal into Data.
private func b(_ str: String) -> Data { Data(str.utf8) }
