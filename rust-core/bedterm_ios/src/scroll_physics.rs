//! Port of `ScrollPhysics.swift` and `BlockListMomentum.swift`.
//!
//! Frame-rate-independent exponential decay for scroll inertia, plus a tiny
//! ring-buffer velocity estimator fed from pan-gesture samples.
//!
//! Both the classic terminal pane and the block-list overlay use pan-flick
//! deceleration. They differ only in constants — the classic pane decays
//! aggressively (settles in ~600ms) while the block list mirrors Warp's
//! desktop feel (decay 0.968 per 8ms). This module captures the shared math
//! without forcing the two coordinate systems (discrete rows vs continuous
//! pixels) into the same scroll controller.

#![allow(dead_code)]

/// Single (timestamp, offset) sample fed by the pan gesture handler. The
/// estimator keeps a tiny ring of these (size 3) so velocity at gesture-end
/// is computed from the most recent motion rather than the full pan history.
#[derive(Clone, Copy, Debug)]
pub struct ScrollSample {
    pub time: f64,
    pub offset_y: f64,
}

impl ScrollSample {
    pub fn new(time: f64, offset_y: f64) -> Self {
        Self { time, offset_y }
    }
}

/// Computes a pan velocity in px/s from the last two scroll samples. Three
/// slots so we get a velocity estimate while the user is mid-pan plus the
/// final two-point estimate at gesture end.
#[derive(Clone, Debug, Default)]
pub struct VelocityEstimator {
    samples: Vec<ScrollSample>,
}

impl VelocityEstimator {
    pub fn new() -> Self {
        Self {
            samples: Vec::with_capacity(3),
        }
    }

    pub fn push(&mut self, sample: ScrollSample) {
        self.samples.push(sample);
        if self.samples.len() > 3 {
            let drop = self.samples.len() - 3;
            self.samples.drain(0..drop);
        }
    }

    pub fn reset(&mut self) {
        self.samples.clear();
    }

    pub fn samples(&self) -> &[ScrollSample] {
        &self.samples
    }

    /// Two-point finite-difference velocity. Returns 0 when we don't have at
    /// least two samples or when the time delta is degenerate.
    pub fn velocity(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let prev = &self.samples[self.samples.len() - 2];
        let last = &self.samples[self.samples.len() - 1];
        let dt = last.time - prev.time;
        if dt <= 0.0001 {
            return 0.0;
        }
        (last.offset_y - prev.offset_y) / dt
    }
}

/// Frame-rate-independent exponential decay for scroll inertia.
#[derive(Clone, Copy, Debug)]
pub struct ScrollPhysics {
    pub velocity: f64,
    pub last_tick: f64,
    /// Multiplier applied once per `decay_interval` seconds.
    pub decay: f64,
    /// Time window (seconds) over which `decay` is applied once.
    pub decay_interval: f64,
}

impl ScrollPhysics {
    pub fn new(decay: f64, decay_interval: f64) -> Self {
        Self {
            velocity: 0.0,
            last_tick: 0.0,
            decay,
            decay_interval,
        }
    }

    /// Advance one tick: apply velocity → displacement, then decay velocity.
    /// Returns the displacement for this frame (points or rows — caller
    /// decides).
    pub fn step(&mut self, now: f64) -> f64 {
        let dt = (now - self.last_tick).max(0.0);
        self.last_tick = now;
        if dt <= 0.0 {
            return 0.0;
        }
        let delta = self.velocity * dt;
        self.velocity *= self.decay.powf(dt / self.decay_interval);
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_from_last_two_samples() {
        let mut est = VelocityEstimator::new();
        est.push(ScrollSample::new(0.0, 0.0));
        est.push(ScrollSample::new(0.01, 10.0));
        assert!((est.velocity() - 1000.0).abs() < 1.0);
    }

    #[test]
    fn velocity_zero_when_insufficient_samples() {
        let mut est = VelocityEstimator::new();
        assert_eq!(est.velocity(), 0.0);
        est.push(ScrollSample::new(0.0, 0.0));
        assert_eq!(est.velocity(), 0.0);
    }

    #[test]
    fn ring_drops_oldest_samples() {
        let mut est = VelocityEstimator::new();
        est.push(ScrollSample::new(0.0, 0.0));
        est.push(ScrollSample::new(0.01, 5.0));
        est.push(ScrollSample::new(0.02, 10.0));
        est.push(ScrollSample::new(0.03, 100.0));
        assert!((est.velocity() - 9000.0).abs() < 1.0);
        assert_eq!(est.samples().len(), 3);
    }

    #[test]
    fn reset_clears_samples() {
        let mut est = VelocityEstimator::new();
        est.push(ScrollSample::new(0.0, 0.0));
        est.push(ScrollSample::new(0.01, 5.0));
        est.reset();
        assert_eq!(est.velocity(), 0.0);
        assert!(est.samples().is_empty());
    }

    #[test]
    fn decay_matches_warp_over_decay_window() {
        let mut physics = ScrollPhysics::new(0.968, 0.008);
        physics.velocity = 1000.0;
        physics.last_tick = 0.0;
        let _ = physics.step(0.008);
        assert!((physics.velocity - 968.0).abs() < 1.0);
    }

    #[test]
    fn offset_advances_by_velocity_times_delta() {
        let mut physics = ScrollPhysics::new(0.968, 0.008);
        physics.velocity = 200.0;
        physics.last_tick = 0.0;
        let delta = physics.step(0.1);
        assert!((delta - 20.0).abs() < 0.001);
    }

    #[test]
    fn zero_dt_returns_zero_displacement() {
        let mut physics = ScrollPhysics::new(0.968, 0.008);
        physics.velocity = 200.0;
        physics.last_tick = 100.0;
        let delta = physics.step(100.0);
        assert_eq!(delta, 0.0);
    }
}
