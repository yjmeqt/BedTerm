// swiftlint:disable sorted_imports
// swiftformat:disable sortImports blankLineAfterImports
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
}

extension ContinuousClock.Instant {
    static var anchor: ContinuousClock.Instant { ContinuousClock().now }
}
