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
        case let (.ctrlPending, .char(letter)):
            self = .idle
            let scalar = letter.lowercased().unicodeScalars.first?.value ?? 0
            if (UnicodeScalar("a").value ... UnicodeScalar("z").value).contains(scalar) {
                let byte = UInt8(scalar & 0x1F)
                return [.bytes(Data([byte])), .visualUnlatch]
            }
            return [.visualUnlatch]
        case (.ctrlPending, .ctrl):
            self = .idle
            return [.visualUnlatch]
        case let (.ctrlPending(startedAt), .tick(now)):
            if now - startedAt > .seconds(3) {
                self = .idle
                return [.visualUnlatch]
            }
            return [.noop]
        default:
            return [.noop]
        }
    }
}
