import Foundation

struct TimeoutError: Error {}

/// Race `operation` against a sleep. Throws `TimeoutError` if the sleep wins;
/// otherwise returns the operation's result. Cancels the loser when either
/// branch finishes so no orphan task lingers.
func withTimeout<T: Sendable>(
    seconds: TimeInterval,
    operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask { try await operation() }
        group.addTask {
            try await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
            throw TimeoutError()
        }
        defer { group.cancelAll() }
        guard let result = try await group.next() else { throw TimeoutError() }
        return result
    }
}
