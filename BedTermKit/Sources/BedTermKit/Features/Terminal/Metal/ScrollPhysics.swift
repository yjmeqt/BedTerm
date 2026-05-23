import Foundation

/// Frame-rate-independent exponential decay for scroll inertia.
///
/// Both the classic terminal pane and the block-list overlay use pan-flick
/// deceleration. They differ only in constants — the classic pane decays
/// aggressively (settles in ~600ms) while the block list mirrors Warp's
/// desktop feel (decay 0.968 per 8ms). This struct captures the shared math
/// without forcing the two coordinate systems (discrete rows vs continuous
/// pixels) into the same scroll controller.
struct ScrollPhysics {
    var velocity: CGFloat = 0
    var lastTick: CFTimeInterval = 0

    /// Multiplier applied once per `decayInterval` seconds.
    let decay: Double
    /// Time window (seconds) over which `decay` is applied once.
    let decayInterval: Double

    /// Advance one tick: apply velocity → displacement, then decay velocity.
    /// Returns the displacement for this frame (points or rows — caller decides).
    mutating func step(now: CFTimeInterval) -> CGFloat {
        let dt = max(0, now - lastTick)
        lastTick = now
        guard dt > 0 else { return 0 }
        let delta = velocity * CGFloat(dt)
        velocity *= CGFloat(pow(decay, dt / decayInterval))
        return delta
    }
}
