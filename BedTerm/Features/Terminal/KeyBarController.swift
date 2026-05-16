import Foundation
import Observation

@MainActor
@Observable
final class KeyBarController {
    private(set) var state: KeyBarState = .idle

    var isPending: Bool {
        if case .ctrlPending = state { true } else { false }
    }

    private let onBytes: (Data) -> Void
    private var tickTask: Task<Void, Never>?

    init(onBytes: @escaping (Data) -> Void) {
        self.onBytes = onBytes
    }

    func handle(_ tap: KeyTap) {
        var local = state
        let outputs = local.reduce(tap, now: ContinuousClock().now)
        state = local
        applyOutputs(outputs)
        scheduleTickIfNeeded()
    }

    private func applyOutputs(_ outputs: [KeyBarOutput]) {
        for output in outputs {
            switch output {
            case .bytes(let data):
                onBytes(data)
            case .visualLatch, .visualUnlatch, .noop:
                // State already reflects the latch via `state` — view observes it.
                break
            }
        }
    }

    private func scheduleTickIfNeeded() {
        if case .ctrlPending = state {
            if tickTask == nil {
                tickTask = Task { [weak self] in
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .milliseconds(250))
                        guard let self else { return }
                        self.tick()
                        if case .idle = self.state { return }
                    }
                }
            }
        } else {
            tickTask?.cancel()
            tickTask = nil
        }
    }

    private func tick() {
        var local = state
        let outputs = local.reduce(
            .tick(ContinuousClock().now),
            now: ContinuousClock().now
        )
        state = local
        applyOutputs(outputs)
        if case .idle = state {
            tickTask?.cancel()
            tickTask = nil
        }
    }
}
