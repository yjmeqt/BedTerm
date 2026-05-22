import XCTest

@testable import BedTermKit

@MainActor
final class SessionSnapshotTests: XCTestCase {
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

}
