import XCTest

@testable import BedTermKit

/// Sanity-check the momentum decay + velocity arithmetic ported from
/// Warp. Pure-value tests — no UIKit, no Metal, no main-actor needed.
final class BlockListMomentumTests: XCTestCase {
    func testVelocityFromLastTwoSamples() {
        var est = VelocityEstimator()
        est.push(ScrollSample(time: 0, offsetY: 0))
        est.push(ScrollSample(time: 0.01, offsetY: 10))
        XCTAssertEqual(est.velocity(), 1000, accuracy: 1)
    }

    func testVelocityZeroWhenInsufficientSamples() {
        var est = VelocityEstimator()
        XCTAssertEqual(est.velocity(), 0)
        est.push(ScrollSample(time: 0, offsetY: 0))
        XCTAssertEqual(est.velocity(), 0)
    }

    func testRingDropsOldestSamples() {
        var est = VelocityEstimator()
        est.push(ScrollSample(time: 0, offsetY: 0))
        est.push(ScrollSample(time: 0.01, offsetY: 5))
        est.push(ScrollSample(time: 0.02, offsetY: 10))
        est.push(ScrollSample(time: 0.03, offsetY: 100))  // fourth sample evicts the first
        // Velocity now from samples 2 and 3 (y=10 → 100 over 0.01s).
        XCTAssertEqual(est.velocity(), 9000, accuracy: 1)
    }

    func testDecayMatchesWarpOverDecayWindow() {
        var state = MomentumState(velocityPxPerSec: 1000, lastTick: 0)
        _ = MomentumState.advance(offset: 0, state: &state, now: 0.008) { ($0, false) }
        XCTAssertEqual(state.velocityPxPerSec, 968, accuracy: 1)
    }

    func testClampStopsMomentum() {
        var state = MomentumState(velocityPxPerSec: 1000, lastTick: 0)
        let (_, done) = MomentumState.advance(offset: 0, state: &state, now: 0.008) { _ in (100, true) }
        XCTAssertTrue(done)
    }

    func testLowVelocityFinishes() {
        var state = MomentumState(velocityPxPerSec: 0.5, lastTick: 0)
        let (_, done) = MomentumState.advance(offset: 0, state: &state, now: 0.008) { ($0, false) }
        XCTAssertTrue(done)
    }

    func testOffsetAdvancesByVelocityTimesDelta() {
        var state = MomentumState(velocityPxPerSec: 200, lastTick: 0)
        let (newY, _) = MomentumState.advance(offset: 50, state: &state, now: 0.1) { ($0, false) }
        // 50 + 200 * 0.1 = 70 (clamp passthrough).
        XCTAssertEqual(newY, 70, accuracy: 0.001)
    }
}
