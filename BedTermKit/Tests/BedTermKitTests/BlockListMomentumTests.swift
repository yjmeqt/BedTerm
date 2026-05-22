import Foundation
import Testing

@testable import BedTermKit

/// Sanity-check the momentum decay + velocity arithmetic ported from
/// Warp. Pure-value tests — no UIKit, no Metal, no main-actor needed.
@Suite("BlockListMomentum")
struct BlockListMomentumTests {
    @Test("velocity from last two samples")
    func velocityFromLastTwoSamples() {
        var est = VelocityEstimator()
        est.push(ScrollSample(time: 0, offsetY: 0))
        est.push(ScrollSample(time: 0.01, offsetY: 10))
        #expect(abs(est.velocity() - 1000) < 1)
    }

    @Test("velocity is zero with fewer than two samples")
    func velocityZeroWhenInsufficientSamples() {
        var est = VelocityEstimator()
        #expect(est.velocity() == 0)
        est.push(ScrollSample(time: 0, offsetY: 0))
        #expect(est.velocity() == 0)
    }

    @Test("ring buffer drops oldest samples past capacity")
    func ringDropsOldestSamples() {
        var est = VelocityEstimator()
        est.push(ScrollSample(time: 0, offsetY: 0))
        est.push(ScrollSample(time: 0.01, offsetY: 5))
        est.push(ScrollSample(time: 0.02, offsetY: 10))
        est.push(ScrollSample(time: 0.03, offsetY: 100))  // fourth sample evicts the first
        // Velocity now from samples 2 and 3 (y=10 → 100 over 0.01s).
        #expect(abs(est.velocity() - 9000) < 1)
    }

    @Test("decay matches Warp over the 8 ms decay window")
    func decayMatchesWarpOverDecayWindow() {
        var state = MomentumState(velocityPxPerSec: 1000, lastTick: 0)
        _ = MomentumState.advance(offset: 0, state: &state, now: 0.008) { ($0, false) }
        #expect(abs(state.velocityPxPerSec - 968) < 1)
    }

    @Test("clamp signal stops momentum")
    func clampStopsMomentum() {
        var state = MomentumState(velocityPxPerSec: 1000, lastTick: 0)
        let (_, done) = MomentumState.advance(offset: 0, state: &state, now: 0.008) { _ in (100, true) }
        #expect(done)
    }

    @Test("low velocity advances finish")
    func lowVelocityFinishes() {
        var state = MomentumState(velocityPxPerSec: 0.5, lastTick: 0)
        let (_, done) = MomentumState.advance(offset: 0, state: &state, now: 0.008) { ($0, false) }
        #expect(done)
    }

    @Test("offset advances by velocity × dt")
    func offsetAdvancesByVelocityTimesDelta() {
        var state = MomentumState(velocityPxPerSec: 200, lastTick: 0)
        let (newY, _) = MomentumState.advance(offset: 50, state: &state, now: 0.1) { ($0, false) }
        // 50 + 200 * 0.1 = 70 (clamp passthrough).
        #expect(abs(newY - 70) < 0.001)
    }
}
