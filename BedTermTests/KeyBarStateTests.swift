// swiftlint:disable sorted_imports
// swiftformat:disable sortImports blankLineAfterImports
import Foundation
import Testing
@testable import BedTerm
// swiftformat:enable sortImports blankLineAfterImports
// swiftlint:enable sorted_imports

@Suite("KeyBarState")
struct KeyBarStateTests {
    @Test("tapping CTRL from idle enters ctrlPending and emits visualLatch")
    func ctrlFromIdleEntersPending() {
        var state = KeyBarState.idle
        let outputs = state.reduce(.ctrl, now: .anchor)
        #expect(outputs == [.visualLatch])
        if case .ctrlPending = state { /* ok */ } else {
            Issue.record("expected ctrlPending, got \(state)")
        }
    }

    @Test("ctrlPending + letter sends Ctrl+letter byte (ASCII & 0x1F) and returns to idle",
          arguments: [
            (Character("c"), UInt8(0x03)),
            (Character("d"), UInt8(0x04)),
            (Character("z"), UInt8(0x1A)),
            (Character("l"), UInt8(0x0C)),
            (Character("a"), UInt8(0x01)),
            (Character("e"), UInt8(0x05)),
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
}

extension ContinuousClock.Instant {
    static var anchor: ContinuousClock.Instant { ContinuousClock().now }
}
