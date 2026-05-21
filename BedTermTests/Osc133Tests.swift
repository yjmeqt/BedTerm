import XCTest

@testable import BedTermKit

final class Osc133Tests: XCTestCase {
    /// `ESC]133;<payload> BEL` — the canonical OSC form used by iTerm2, kitty,
    /// VSCode, and WezTerm. Matches what the bundled shell-integration script
    /// emits.
    private func osc(_ payload: String) -> Data {
        var bytes: [UInt8] = [0x1B, 0x5D]  // ESC ]
        bytes.append(contentsOf: payload.utf8)
        bytes.append(0x07)  // BEL
        return Data(bytes)
    }

    func testEmptyQueueWhenIdle() {
        let core = TerminalCore(cols: 80, rows: 24)
        XCTAssertNil(core.popOsc133Event())
        XCTAssertEqual(core.drainOsc133Events().count, 0)
    }

    func testPromptStartWithoutAttrs() {
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;A"))
        XCTAssertEqual(core.popOsc133Event(), .promptStart(attrs: [:]))
    }

    func testFullLifecycle() {
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;A"))
        core.feed(Data("$ ".utf8))
        core.feed(osc("133;B"))
        core.feed(Data("ls\n".utf8))
        core.feed(osc("133;C"))
        core.feed(Data("file1 file2\n".utf8))
        core.feed(osc("133;D;0"))
        XCTAssertEqual(
            core.drainOsc133Events(),
            [
                .promptStart(attrs: [:]),
                .commandStart(attrs: [:]),
                .outputStart(attrs: [:]),
                .commandEnd(exitCode: 0, attrs: [:])
            ])
    }

    func testPromptStartWithITerm2StyleAttrs() {
        // iTerm2 ships `user-host` and `current-dir` extension params.
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;A;user-host=alice@host;current-dir=/Users/alice"))
        XCTAssertEqual(
            core.popOsc133Event(),
            .promptStart(attrs: [
                "user-host": "alice@host",
                "current-dir": "/Users/alice"
            ])
        )
    }

    func testOutputStartWithCmdAttr() {
        // Shape our bundled enriched script will emit: command text in `cmd`.
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;C;cmd=bHM="))
        XCTAssertEqual(
            core.popOsc133Event(),
            .outputStart(attrs: ["cmd": "bHM="])
        )
    }

    func testCommandEndWithExitAndAttrs() {
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;D;0;dur=1234;cwd=L1VzZXJzL2FsaWNl"))
        XCTAssertEqual(
            core.popOsc133Event(),
            .commandEnd(
                exitCode: 0,
                attrs: [
                    "dur": "1234",
                    "cwd": "L1VzZXJzL2FsaWNl"
                ])
        )
    }

    func testNonzeroExitCode() {
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;D;127"))
        XCTAssertEqual(core.popOsc133Event(), .commandEnd(exitCode: 127, attrs: [:]))
    }

    func testCommandEndWithoutExitCode() {
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;D"))
        XCTAssertEqual(core.popOsc133Event(), .commandEnd(exitCode: nil, attrs: [:]))
    }

    func testCommandEndKeyEqualsValueAtPositionTwoSkipsExitParse() {
        // `133;D;err=14` — positional-2 is a key=value, not an int; exit_code
        // should be nil and attrs start at position 2.
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;D;err=14"))
        XCTAssertEqual(
            core.popOsc133Event(),
            .commandEnd(exitCode: nil, attrs: ["err": "14"])
        )
    }

    func testOtherOscsDontEmitEvents() {
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("0;hello"))
        core.feed(osc("4;1;#ff0000"))
        core.feed(osc("8;;https://example.com"))
        XCTAssertEqual(core.drainOsc133Events().count, 0)
    }

    func testAttrsAreCopiedNotBorrowed() {
        // Drain two events; the scratch buffer is overwritten between them,
        // but the first event's attrs should still be valid because Swift
        // copied them during init(raw:).
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(osc("133;A;current-dir=/first"))
        core.feed(osc("133;A;current-dir=/second"))
        let first = core.popOsc133Event()
        let second = core.popOsc133Event()
        XCTAssertEqual(first, .promptStart(attrs: ["current-dir": "/first"]))
        XCTAssertEqual(second, .promptStart(attrs: ["current-dir": "/second"]))
    }

    func testEventsSurviveSplitFeeds() {
        let core = TerminalCore(cols: 80, rows: 24)
        core.feed(Data([0x1B, 0x5D, 0x31]))  // ESC ] '1'
        core.feed(Data([0x33, 0x33, 0x3B]))  // '3' '3' ';'
        core.feed(Data([0x41, 0x07]))  // 'A' BEL
        XCTAssertEqual(core.popOsc133Event(), .promptStart(attrs: [:]))
    }
}
