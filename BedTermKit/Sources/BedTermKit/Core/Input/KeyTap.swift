import Foundation

public enum KeyTap: Equatable, Sendable {
    case ctrl
    case tab
    case esc
    case up
    case down
    case left
    case right
    case char(Character)
    case tick(ContinuousClock.Instant)
}

public enum KeyBarOutput: Equatable, Sendable {
    case bytes(Data)
    case visualLatch
    case visualUnlatch
    case noop
}
