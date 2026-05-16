import Foundation
import Testing

@testable import BedTerm

@Suite("KeyBarState")
struct KeyBarStateTests {
    @Test("tapping CTRL from idle enters ctrlPending and emits visualLatch")
    func ctrlFromIdleEntersPending() {
        var state = KeyBarState.idle
        let outputs = state.reduce(.ctrl, now: .anchor)
        #expect(outputs == [.visualLatch])
        if case .ctrlPending = state { /* ok */
        } else {
            Issue.record("expected ctrlPending, got \(state)")
        }
    }

    @Test(
        "ctrlPending + letter sends Ctrl+letter byte (ASCII & 0x1F) and returns to idle",
        arguments: [
            (Character("c"), UInt8(0x03)),
            (Character("d"), UInt8(0x04)),
            (Character("z"), UInt8(0x1A)),
            (Character("l"), UInt8(0x0C)),
            (Character("a"), UInt8(0x01)),
            (Character("e"), UInt8(0x05))
        ])
    func ctrlPendingPlusLetter(_ letter: Character, _ expectedByte: UInt8) {
        var state = KeyBarState.ctrlPending(startedAt: .anchor)
        let outputs = state.reduce(.char(letter), now: .anchor)
        #expect(outputs == [.bytes(Data([expectedByte])), .visualUnlatch])
        #expect(state == .idle)
    }

    @Test("ctrl + uppercase letter still maps to the same control byte")
    func ctrlPendingPlusUppercase() {
        var state = KeyBarState.ctrlPending(startedAt: .anchor)
        let outputs = state.reduce(.char("C"), now: .anchor)
        #expect(outputs == [.bytes(Data([0x03])), .visualUnlatch])
        #expect(state == .idle)
    }

    @Test("ctrl + non-letter character is a no-op and exits pending")
    func ctrlPendingPlusDigit() {
        var state = KeyBarState.ctrlPending(startedAt: .anchor)
        let outputs = state.reduce(.char("5"), now: .anchor)
        #expect(outputs == [.visualUnlatch])
        #expect(state == .idle)
    }

    @Test("tapping CTRL again while pending cancels pending (R4 ctrl_tap_again)")
    func ctrlPendingPlusCtrlCancels() {
        var state = KeyBarState.ctrlPending(startedAt: .anchor)
        let outputs = state.reduce(.ctrl, now: .anchor)
        #expect(outputs == [.visualUnlatch])
        #expect(state == .idle)
    }

    @Test("tick after >3s in pending cancels pending (R4 ctrl_timeout)")
    func ctrlPendingTimeout() {
        let start = ContinuousClock().now
        var state = KeyBarState.ctrlPending(startedAt: start)
        let outputs = state.reduce(.tick(start.advanced(by: .seconds(4))), now: start.advanced(by: .seconds(4)))
        #expect(outputs == [.visualUnlatch])
        #expect(state == .idle)
    }

    @Test("tick within 3s in pending is a no-op")
    func ctrlPendingTickEarly() {
        let start = ContinuousClock().now
        var state = KeyBarState.ctrlPending(startedAt: start)
        let outputs = state.reduce(.tick(start.advanced(by: .seconds(1))), now: start.advanced(by: .seconds(1)))
        #expect(outputs == [.noop])
        #expect(state == .ctrlPending(startedAt: start))
    }
}

extension ContinuousClock.Instant {
    static var anchor: ContinuousClock.Instant { ContinuousClock().now }
}
