import Foundation

/// Single (timestamp, offset) sample fed by the pan gesture handler.
/// The estimator keeps a tiny ring of these (size 3) so velocity at
/// gesture-end is computed from the most recent motion rather than the
/// full pan history.
struct ScrollSample {
    let time: CFTimeInterval
    let offsetY: CGFloat
}

/// Computes a pan velocity in px/s from the last two scroll samples.
/// Three slots so we get a velocity estimate while the user is mid-pan
/// plus the final two-point estimate at gesture end.
struct VelocityEstimator {
    private(set) var samples: [ScrollSample] = []

    mutating func push(_ sample: ScrollSample) {
        samples.append(sample)
        if samples.count > 3 { samples.removeFirst(samples.count - 3) }
    }

    mutating func reset() { samples.removeAll(keepingCapacity: true) }

    /// Two-point finite-difference velocity. Returns 0 when we don't
    /// have at least two samples or when the time delta is degenerate.
    func velocity() -> CGFloat {
        guard samples.count >= 2 else { return 0 }
        let prev = samples[samples.count - 2]
        let last = samples[samples.count - 1]
        let dt = last.time - prev.time
        guard dt > 0.0001 else { return 0 }
        return (last.offsetY - prev.offsetY) / CGFloat(dt)
    }
}
