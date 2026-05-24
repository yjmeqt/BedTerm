import Foundation
import Testing

@testable import BedTermKit

@Suite("ScrollPhysics")
struct ScrollPhysicsTests {
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
        est.push(ScrollSample(time: 0.03, offsetY: 100))
        #expect(abs(est.velocity() - 9000) < 1)
    }

    @Test("decay matches Warp over the 8 ms decay window")
    func decayMatchesWarpOverDecayWindow() {
        var physics = ScrollPhysics(decay: 0.968, decayInterval: 0.008)
        physics.velocity = 1000
        physics.lastTick = 0
        _ = physics.step(now: 0.008)
        #expect(abs(physics.velocity - 968) < 1)
    }

    @Test("offset advances by velocity × dt")
    func offsetAdvancesByVelocityTimesDelta() {
        var physics = ScrollPhysics(decay: 0.968, decayInterval: 0.008)
        physics.velocity = 200
        physics.lastTick = 0
        let delta = physics.step(now: 0.1)
        #expect(abs(delta - 20) < 0.001)
    }

    @Test("step with zero dt returns zero displacement")
    func zeroDtReturnsZero() {
        var physics = ScrollPhysics(decay: 0.968, decayInterval: 0.008)
        physics.velocity = 200
        physics.lastTick = 100
        let delta = physics.step(now: 100)  // dt = 0
        #expect(delta == 0)
    }
}
