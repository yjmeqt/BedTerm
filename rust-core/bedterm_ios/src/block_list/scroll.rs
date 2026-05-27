//! Port of `BlockListContainerView+Scroll.swift`.
//!
//! Pure-math state machine for the block list's scroll anchor +
//! deceleration. Two clean things:
//!
//!   * `ScrollPosition` — `followsBottom` / `fixedAt(y)` (mirrors the
//!     Swift enum).
//!   * `BlockListScrollState` — owns the offset, the pan-velocity
//!     estimator, and the active `ScrollPhysics`. The owning VC drives
//!     it from the pan gesture handler + a CADisplayLink tick.
//!
//! The metal view (`crate::metal_view`) has its **own** CADisplayLink-
//! driven inertia from W3.H4 used while the classic single-pane mode
//! is active. The block list VC keeps a separate link because it has to
//! advance per-block layout (including the sticky band) on every tick,
//! which the metal view's inertia loop knows nothing about — they live
//! at different layers and shouldn't share. The metal view's inertia is
//! quiescent while `.blockList` mode is active anyway (it doesn't own
//! the gesture in that mode).

#![allow(dead_code)]

use crate::geometry::CGFloat;
use crate::scroll_physics::{ScrollPhysics, ScrollSample, VelocityEstimator};

/// Tolerance (in points) for snapping back to `.followsBottom` once the
/// user scrolls within this distance of the bottom edge.
pub const PIN_TOLERANCE_PT: CGFloat = 24.0;

/// Scroll-physics constants matching the Swift `blockListScrollPhysics`
/// (Warp-style desktop decay).
pub fn block_list_scroll_physics() -> ScrollPhysics {
    ScrollPhysics::new(0.968, 0.008)
}

/// Mirrors `ScrollPosition` in the Swift extension.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ScrollPosition {
    #[default]
    FollowsBottom,
    FixedAt(CGFloat),
}

/// Owning state for the block list's scroll. Lives inside the VC's
/// ivars (behind `RefCell`s). Methods are pure — no UIKit calls — so
/// the unit tests live with the rest of the offline-CLI tests.
#[derive(Default)]
pub struct BlockListScrollState {
    pub content_offset_y: CGFloat,
    pub content_height: CGFloat,
    pub viewport_height: CGFloat,
    pub pan_start_offset_y: CGFloat,
    pub velocity_estimator: VelocityEstimator,
    pub scroll_physics: Option<ScrollPhysics>,
    pub scroll_position: ScrollPosition,
    pub last_block_count: usize,
}

impl BlockListScrollState {
    pub fn max_offset_y(&self) -> CGFloat {
        (self.content_height - self.viewport_height).max(0.0)
    }

    pub fn clamp_offset(&self, raw: CGFloat) -> (CGFloat, bool) {
        let m = self.max_offset_y();
        let c = raw.max(0.0).min(m);
        (c, (c - raw).abs() > f64::EPSILON)
    }

    pub fn is_at_bottom_edge(&self) -> bool {
        self.content_offset_y >= self.max_offset_y() - PIN_TOLERANCE_PT
    }

    /// Apply the current anchor to `content_offset_y`. Skipped during an
    /// active pan / momentum so the gesture owns the offset.
    pub fn apply_scroll_position(&mut self, pan_active: bool) {
        if self.viewport_height <= 0.0 || pan_active || self.scroll_physics.is_some() {
            return;
        }
        let max_off = self.max_offset_y();
        match self.scroll_position {
            ScrollPosition::FollowsBottom => {
                self.content_offset_y = max_off;
            }
            ScrollPosition::FixedAt(anchor) => {
                let clamped = anchor.max(0.0).min(max_off);
                if (self.content_offset_y - clamped).abs() > 0.5 {
                    self.content_offset_y = clamped;
                }
            }
        }
    }

    /// Pan-began: stop momentum, snap anchor to current offset.
    pub fn begin_pan(&mut self, now: f64) {
        self.scroll_physics = None;
        self.pan_start_offset_y = self.content_offset_y;
        self.scroll_position = ScrollPosition::FixedAt(self.content_offset_y);
        self.velocity_estimator.reset();
        self.velocity_estimator
            .push(ScrollSample::new(now, self.content_offset_y));
    }

    /// Pan-changed: translation is in points (drag distance, positive
    /// = finger moved down). The scroll position therefore moves by
    /// `-translation`.
    pub fn update_pan(&mut self, translation_y_pt: CGFloat, now: f64) {
        let (clamped, _) = self.clamp_offset(self.pan_start_offset_y - translation_y_pt);
        self.content_offset_y = clamped;
        self.velocity_estimator
            .push(ScrollSample::new(now, self.content_offset_y));
        self.scroll_position = if self.is_at_bottom_edge() {
            ScrollPosition::FollowsBottom
        } else {
            ScrollPosition::FixedAt(self.content_offset_y)
        };
    }

    /// Pan-ended: kick off inertia if velocity > threshold, otherwise
    /// snap back to `followsBottom` when we landed at the edge.
    pub fn end_pan(&mut self, now: f64) {
        let v = self.velocity_estimator.velocity();
        if v.abs() > 50.0 {
            let mut p = block_list_scroll_physics();
            // Pan velocity is in pixels-per-second of content offset.
            // Apply to the physics velocity directly.
            p.velocity = v;
            p.last_tick = now;
            self.scroll_physics = Some(p);
        } else if self.is_at_bottom_edge() {
            self.scroll_position = ScrollPosition::FollowsBottom;
        }
    }

    /// One step of the CADisplayLink-driven momentum. Returns `true`
    /// when momentum is exhausted and the VC should stop the link.
    pub fn advance_momentum(&mut self, now: f64) -> bool {
        let Some(mut physics) = self.scroll_physics else {
            return true;
        };
        let delta = physics.step(now);
        let (clamped, hit_edge) = self.clamp_offset(self.content_offset_y + delta);
        self.content_offset_y = clamped;
        if hit_edge || physics.velocity.abs() < 1.0 {
            self.scroll_physics = None;
            self.scroll_position = if self.is_at_bottom_edge() {
                ScrollPosition::FollowsBottom
            } else {
                ScrollPosition::FixedAt(clamped)
            };
            true
        } else {
            self.scroll_physics = Some(physics);
            false
        }
    }

    /// Accessibility scroll — pages by 80 % of the viewport.
    pub fn accessibility_scroll(&mut self, direction_down: bool) -> bool {
        let step = self.viewport_height * 0.8;
        let (clamped, _) = self.clamp_offset(if direction_down {
            self.content_offset_y + step
        } else {
            self.content_offset_y - step
        });
        self.content_offset_y = clamped;
        self.scroll_position = if self.is_at_bottom_edge() {
            ScrollPosition::FollowsBottom
        } else {
            ScrollPosition::FixedAt(self.content_offset_y)
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_offset_clamps_to_zero_when_content_fits() {
        let mut s = BlockListScrollState {
            viewport_height: 800.0,
            content_height: 400.0,
            ..Default::default()
        };
        assert_eq!(s.max_offset_y(), 0.0);
        let (c, _) = s.clamp_offset(123.0);
        assert_eq!(c, 0.0);
        s.apply_scroll_position(false);
        assert_eq!(s.content_offset_y, 0.0);
    }

    #[test]
    fn follows_bottom_pins_at_max_offset() {
        let mut s = BlockListScrollState {
            viewport_height: 800.0,
            content_height: 2000.0,
            scroll_position: ScrollPosition::FollowsBottom,
            ..Default::default()
        };
        s.apply_scroll_position(false);
        assert_eq!(s.content_offset_y, 1200.0);
    }

    #[test]
    fn pan_drags_then_decays_into_inertia() {
        let mut s = BlockListScrollState {
            viewport_height: 800.0,
            content_height: 4000.0,
            ..Default::default()
        };
        s.begin_pan(0.0);
        s.update_pan(-300.0, 0.05); // finger swiped up → offset increases
        assert!(s.content_offset_y > 0.0);
        s.end_pan(0.05);
        assert!(s.scroll_physics.is_some());
    }
}
