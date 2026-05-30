//! Pure-Rust layout constants for the W24b `list_row` component.
//!
//! Kept outside the iOS-gated `components` submodule so the parity tests
//! that lock these values down to the SwiftUI `HostRow` rhythm can run
//! on a macOS host (`cargo test -p bedterm_ios`).

// On non-iOS hosts the constants and helper are only consumed by tests,
// so silence dead-code warnings at module scope.
#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

use crate::geometry::CGFloat;

/// Geometry constants used by the [`crate::design_system::components::list_row`]
/// factory. Locked to parity with `HostRow.swift` so flipping the
/// `useRustHostsList` flag doesn't shift the visual rhythm.
pub struct ListRowMetrics;

impl ListRowMetrics {
    /// Corner radius applied to the row's CALayer.
    pub const CORNER_RADIUS: CGFloat = 10.0;
    /// CALayer border width.
    pub const BORDER_WIDTH: CGFloat = 1.0;
    /// Horizontal padding inside the row (leading + trailing).
    pub const HORIZONTAL_PADDING: CGFloat = 16.0;
    /// Vertical padding inside the row (top + bottom).
    pub const VERTICAL_PADDING: CGFloat = 12.0;
    /// Vertical spacing between the title and subtitle labels.
    pub const TITLE_SUBTITLE_SPACING: CGFloat = 2.0;
    /// Horizontal spacing between the text stack and trailing accessory.
    pub const TEXT_TO_ACCESSORY_SPACING: CGFloat = 12.0;
    /// SF symbol point size for the trailing chevron.
    pub const CHEVRON_POINT_SIZE: CGFloat = 13.0;
    /// Side length of the bordered square wrapping the auth badge.
    /// Mirrors SwiftUI `HostRow.authBadge`'s `.frame(width: 24, height: 24)`.
    pub const BADGE_SIZE: CGFloat = 24.0;
    /// Corner radius of the badge square — matches `RoundedRectangle(cornerRadius: 6)`.
    pub const BADGE_CORNER_RADIUS: CGFloat = 6.0;
    /// SF symbol point size for the badge glyph — `.caption2.weight(.semibold)`
    /// is ~11pt in SwiftUI.
    pub const BADGE_GLYPH_POINT_SIZE: CGFloat = 11.0;

    /// Computed minimum row height; used by tests as a regression fence.
    /// Reflects the SwiftUI `HostRow` parity sizing: `.callout` (16pt)
    /// title + `.footnote` (13pt) subtitle.
    #[allow(dead_code)] // pure helper consumed only by unit tests.
    pub fn min_row_height(has_subtitle: bool) -> CGFloat {
        let title_h: CGFloat = 21.0; // ~16pt callout line-height.
        let subtitle_h: CGFloat = 17.0; // ~13pt footnote line-height.
        let content = if has_subtitle {
            title_h + Self::TITLE_SUBTITLE_SPACING + subtitle_h
        } else {
            title_h
        };
        content + Self::VERTICAL_PADDING * 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_row_height_grows_with_subtitle() {
        let no_sub = ListRowMetrics::min_row_height(false);
        let with_sub = ListRowMetrics::min_row_height(true);
        assert!(with_sub > no_sub);
    }

    #[test]
    fn min_row_height_uses_padding_twice() {
        let h = ListRowMetrics::min_row_height(false);
        let expected = 21.0 + ListRowMetrics::VERTICAL_PADDING * 2.0;
        assert!((h - expected).abs() < 1e-6, "min height = {h}");
    }
}
