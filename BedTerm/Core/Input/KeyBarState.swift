import Foundation

public enum KeyBarState: Equatable {
    case idle
    case ctrlPending(startedAt: ContinuousClock.Instant)

    // swiftlint:disable cyclomatic_complexity
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
        case (_, .tab):
            let wasPending = self.isPending
            self = .idle
            var out: [KeyBarOutput] = [.bytes(Data([0x09]))]
            if wasPending { out.append(.visualUnlatch) }
            return out
        case (_, .esc):
            let wasPending = self.isPending
            self = .idle
            var out: [KeyBarOutput] = [.bytes(Data([0x1B]))]
            if wasPending { out.append(.visualUnlatch) }
            return out
        case (_, .up):
            return self.emitArrow(0x41)
        case (_, .down):
            return self.emitArrow(0x42)
        case (_, .right):
            return self.emitArrow(0x43)
        case (_, .left):
            return self.emitArrow(0x44)
        default:
            return [.noop]
        }
    }

    // swiftlint:enable cyclomatic_complexity

    private var isPending: Bool {
        if case .ctrlPending = self { true } else { false }
    }

    private mutating func emitArrow(_ final: UInt8) -> [KeyBarOutput] {
        let wasPending = self.isPending
        self = .idle
        var out: [KeyBarOutput] = [.bytes(Data([0x1B, 0x5B, final]))]
        if wasPending { out.append(.visualUnlatch) }
        return out
    }
}
