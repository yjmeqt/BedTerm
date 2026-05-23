import Testing

@testable import BedTermKit

@Suite("SessionSnapshot helpers")
@MainActor
struct SessionSnapshotTests {
    // MARK: - Resume command quoting

    @Test
    func buildResumeCommandQuotesPath() {
        #expect(KilledSessionDetailScreen.buildResumeCommand(cwd: "/home/yj") == "cd '/home/yj'\n")
    }

    @Test
    func buildResumeCommandEscapesSingleQuotes() {
        #expect(
            KilledSessionDetailScreen.buildResumeCommand(cwd: "/tmp/it's mine")
                == "cd '/tmp/it'\\''s mine'\n"
        )
    }

    // MARK: - Path abbreviation

    @Test
    func abbreviateLinuxHome() {
        #expect(SessionSnapshotPathAbbreviator.abbreviate("/home/yj/code") == "~/code")
        #expect(SessionSnapshotPathAbbreviator.abbreviate("/home/yj") == "~")
    }

    @Test
    func abbreviateMacHome() {
        #expect(SessionSnapshotPathAbbreviator.abbreviate("/Users/yj/code") == "~/code")
    }

    @Test
    func abbreviateRoot() {
        #expect(SessionSnapshotPathAbbreviator.abbreviate("/root/log") == "~/log")
        #expect(SessionSnapshotPathAbbreviator.abbreviate("/root") == "~")
    }

    @Test
    func abbreviatePassesThroughUnknown() {
        #expect(SessionSnapshotPathAbbreviator.abbreviate("/var/log") == "/var/log")
    }

    // MARK: - Kill reason copy

    @Test
    func killReasonLocalizedReasonNonEmpty() {
        let reasons: [SessionSnapshot.KillReason] = [
            .userKilled, .remoteLogout, .networkDrop, .appRelaunch, .swapEvicted
        ]
        for reason in reasons {
            #expect(!reason.localizedReason.isEmpty)
        }
    }
}
