import Foundation
import Testing

@testable import BedTermKit

@Suite("KilledSession restore")
@MainActor
struct KilledSessionRestoreTests {
    private func tempDBURL() -> URL {
        FileManager.default.temporaryDirectory
            .appending(path: "bedterm-restore-test-\(UUID().uuidString).sqlite")
    }

    /// `openReplay` must return `nil` for a snapshot that was never inserted.
    @Test
    func openReplayReturnsNilForUnknownSnapshot() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }
        let handle = try #require(PersistenceHandle.open(at: url))
        #expect(handle.openReplay(snapshotID: UUID()) == nil)
    }

    /// Feeds one complete DCS-JSON block through a `TerminalCore` that has
    /// `PersistenceHandle.attach` called on it, then calls `openReplay` and
    /// asserts a non-nil `TerminalCore` with one block comes back.
    @Test
    func openReplayReturnsTerminalCoreAfterBlockCapture() throws {
        let url = tempDBURL()
        defer { try? FileManager.default.removeItem(at: url) }

        let handle = try #require(PersistenceHandle.open(at: url))
        let snapshotID = UUID()
        let hostID = UUID()

        let core = TerminalCore(cols: 80, rows: 24)
        handle.attach(terminal: core, snapshotID: snapshotID, hostID: hostID)

        core.feed(dcsBlock(pwd: "/tmp", command: "echo hi", output: b("hi\n"), exit: 0))
        // A second Precmd seals the first block via the sink.
        core.feed(dcsFrame(json: #"{"hook":"Precmd","value":{"pwd":"/tmp"}}"#))

        handle.recordKill(
            snapshotID: snapshotID,
            reason: .userKilled,
            lastCwd: "/tmp",
            lastCommand: "echo hi",
            lastExitCode: 0
        )

        let replayCore = try #require(handle.openReplay(snapshotID: snapshotID))
        #expect(replayCore.blockCount == 1)
    }
}

// MARK: - DCS helpers

/// Build a hex-encoded DCS frame: `ESC P $ d <hex(json)> ST` (ST = 0x9C).
private func dcsFrame(json: String) -> Data {
    var bytes: [UInt8] = [0x1B, UInt8(ascii: "P"), UInt8(ascii: "$"), UInt8(ascii: "d")]
    for byte in json.utf8 {
        let hi = (byte >> 4) & 0x0F
        let lo = byte & 0x0F
        bytes.append(hi < 10 ? hi + 48 : hi + 87)
        bytes.append(lo < 10 ? lo + 48 : lo + 87)
    }
    bytes.append(0x9C)
    return Data(bytes)
}

private func dcsBlock(pwd: String, command: String, output: Data, exit: Int) -> Data {
    var data = Data()
    data.append(dcsFrame(json: #"{"hook":"Precmd","value":{"pwd":"\#(pwd)"}}"#))
    data.append(dcsFrame(json: #"{"hook":"Preexec","value":{"command":"\#(command)"}}"#))
    data.append(output)
    data.append(dcsFrame(json: #"{"hook":"CommandFinished","value":{"exit_code":\#(exit)}}"#))
    return data
}

private func b(_ str: String) -> Data { Data(str.utf8) }
