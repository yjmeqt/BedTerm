import Foundation

public enum KeyBarState: Equatable {
    case idle
    case ctrlPending(startedAt: ContinuousClock.Instant)

    /// Reducer. Mutates `self` and returns the outputs to apply.
    public mutating func reduce(_ tap: KeyTap, now: ContinuousClock.Instant) -> [KeyBarOutput] {
        switch (self, tap) {
        case (.idle, .ctrl):
            self = .ctrlPending(startedAt: now)
            return [.visualLatch]
        default:
            return [.noop]
        }
    }
}
