import Foundation
import QuartzCore

/// Single (timestamp, offset) sample fed by the pan gesture handler.
/// The estimator keeps a tiny ring of these (size 3) so velocity at
/// gesture-end is computed from the most recent motion rather than the
/// full pan history — feels right when the user briefly pauses before
/// flicking.
struct ScrollSample {
    let time: CFTimeInterval
    let offsetY: CGFloat
}

/// Inertial scroll state mirroring Warp's `MomentumScroll` model.
/// Decay exponent matches Warp's `MOMENTUM_DECAY = 0.968` over an
/// 8 ms decay window so the feel is identical to the desktop app's
/// flick deceleration.
struct MomentumState {
    var velocityPxPerSec: CGFloat
    var lastTick: CFTimeInterval

    /// Per-window decay factor. `velocity *= 0.968` each 8 ms.
    static let decay: Double = 0.968
    /// Window over which `decay` applies — anchored at Warp's sample
    /// rate so frame-rate independence falls out of the math.
    static let decayInterval: Double = 0.008

    /// Advance `offset` by `velocity * dt`, then decay velocity.
    /// `clamp` is invoked to enforce content-range bounds (top/bottom)
    /// and reports `hitEdge=true` when the raw offset overflowed.
    ///
    /// Returns `(newOffset, done)`. `done=true` means the caller
    /// should drop the momentum state — either velocity has decayed
    /// below 1 px/s or we've reached an edge.
    static func advance(
        offset: CGFloat,
        state: inout MomentumState,
        now: CFTimeInterval,
        clamp: (CGFloat) -> (clamped: CGFloat, hitEdge: Bool)
    ) -> (offset: CGFloat, done: Bool) {
        let dt = max(0, now - state.lastTick)
        state.lastTick = now
        let raw = offset + state.velocityPxPerSec * CGFloat(dt)
        let (clamped, hitEdge) = clamp(raw)
        // Frame-rate-independent decay: equivalent to multiplying
        // velocity by `decay` once per `decayInterval` over `dt`.
        let factor = pow(decay, dt / decayInterval)
        state.velocityPxPerSec *= CGFloat(factor)
        let done = hitEdge || abs(state.velocityPxPerSec) < 1.0
        return (clamped, done)
    }
}

/// Computes a pan velocity in px/s from the last two scroll samples.
/// Three slots so we get a velocity estimate while the user is mid-pan
/// (for diagnostics / future use) plus the final two-point estimate
/// at gesture end.
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
