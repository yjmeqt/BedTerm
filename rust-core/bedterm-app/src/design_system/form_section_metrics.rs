//! Pure-Rust layout constants for the `form_section` / `form_card`
//! components. Kept outside the iOS-gated `components` submodule so
//! parity tests run on the macOS host.
//!
//! Locked to parity with SwiftUI `ShadcnCard` in
//! `BedTermKit/Sources/BedTermKit/Core/UI/ShadcnPrimitives.swift` so the
//! Rust connect-form VC stays visually identical when the
//! `useRustConnectForm` flag is on.

#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

use crate::geometry::CGFloat;

/// Geometry constants used by the
/// [`crate::design_system::components::form_section`] /
/// `form_card` factories.
pub struct FormSectionMetrics;

impl FormSectionMetrics {
    /// Corner radius applied to the card layer.
    /// Mirrors `RoundedRectangle(cornerRadius: 10)` in `ShadcnCard`.
    pub const CARD_CORNER_RADIUS: CGFloat = 10.0;
    /// CALayer border width — `.stroke(..., lineWidth: 1)`.
    pub const CARD_BORDER_WIDTH: CGFloat = 1.0;
    /// Padding inside the card on all four edges — `.padding(16)`.
    pub const CARD_PADDING: CGFloat = 16.0;
    /// Spacing between the title and the description inside the card
    /// header — `VStack(alignment: .leading, spacing: 4)`.
    pub const HEADER_TITLE_DESCRIPTION_SPACING: CGFloat = 4.0;
    /// Spacing between the header block and the first field row —
    /// outer `VStack(alignment: .leading, spacing: 16)`.
    pub const HEADER_TO_ROWS_SPACING: CGFloat = 16.0;
    /// Vertical spacing between rows inside the card —
    /// `VStack(spacing: 14)` in `ShadcnCard`.
    pub const ROW_SPACING: CGFloat = 14.0;
    /// Legacy inset-grouped section corner radius (kept for the old
    /// `form_section` API used by the Settings VC).
    pub const LEGACY_CORNER_RADIUS: CGFloat = 8.0;
    /// Legacy inset-grouped section border width.
    pub const LEGACY_BORDER_WIDTH: CGFloat = 0.5;

    // ---- External-header card (classic iOS section style) ------------
    /// Gap between the uppercase section header and the card below it.
    pub const EXTERNAL_HEADER_TO_CARD_SPACING: CGFloat = 8.0;
    /// Gap between the card and the description sentence below it.
    pub const CARD_TO_EXTERNAL_DESCRIPTION_SPACING: CGFloat = 8.0;
    /// Horizontal indent (in points) of the external section header /
    /// description relative to the card edge. Matches the card's internal
    /// horizontal padding so headers align with the leading edge of in-card
    /// text.
    pub const EXTERNAL_HEADER_INSET: CGFloat = 16.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_header_inset_matches_card_padding() {
        // External section headers must align with the leading edge of
        // in-card text — i.e. with the card's internal horizontal padding.
        assert_eq!(
            FormSectionMetrics::EXTERNAL_HEADER_INSET,
            FormSectionMetrics::CARD_PADDING,
        );
    }
}
