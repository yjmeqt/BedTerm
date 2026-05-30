//! Pure-Rust layout constants for the W24c `segmented_control` component.

#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

use crate::geometry::CGFloat;

/// Geometry constants used by the
/// [`crate::design_system::components::segmented_control`] factory.
/// Locked to parity with the SwiftUI `ShadcnSegmented` tab in
/// `ConnectionFormScreen.swift`.
pub struct SegmentedControlMetrics;

impl SegmentedControlMetrics {
    /// Outer corner radius (the rounded "pill" enclosing both tabs).
    pub const OUTER_CORNER_RADIUS: CGFloat = 8.0;
    /// Intrinsic height of the control (matches `ShadcnSegmented`).
    pub const HEIGHT: CGFloat = 34.0;
    /// Inset between the outer border and the selected tab background.
    pub const SELECTION_INSET: CGFloat = 3.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_inset_is_positive_and_small() {
        // Sanity guard — a value of 0 would render the selection background
        // flush with the outer border, and a value >= height/2 would
        // collapse the visible tab to nothing.
        let inset = SegmentedControlMetrics::SELECTION_INSET;
        assert!(inset > 0.0, "selection inset must be positive");
        assert!(
            inset * 2.0 < SegmentedControlMetrics::HEIGHT,
            "selection inset eats more than the control height"
        );
    }
}
