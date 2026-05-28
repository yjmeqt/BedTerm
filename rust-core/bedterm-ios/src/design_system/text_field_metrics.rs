//! Pure-Rust layout constants for the W24c `text_field` /
//! `secure_text_field` components.
//!
//! Kept outside the iOS-gated `components` submodule so parity tests
//! that lock these values down to the SwiftUI `ShadcnTextField` rhythm
//! run on a macOS host (`cargo test -p bedterm_ios`).

#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

use crate::geometry::CGFloat;

/// Geometry constants used by the
/// [`crate::design_system::components::text_field`] /
/// [`crate::design_system::components::secure_text_field`] factories.
/// Locked to parity with `ShadcnTextField` so flipping
/// `useRustConnectForm` doesn't shift the visual rhythm.
pub struct TextFieldMetrics;

impl TextFieldMetrics {
    /// Corner radius applied to the field's CALayer.
    pub const CORNER_RADIUS: CGFloat = 8.0;
    /// CALayer border width.
    pub const BORDER_WIDTH: CGFloat = 1.0;
    /// Horizontal padding inside the field (leading + trailing).
    pub const HORIZONTAL_PADDING: CGFloat = 12.0;
    /// Intrinsic field height; matches the SwiftUI `ShadcnTextField` value.
    pub const FIELD_HEIGHT: CGFloat = 36.0;
    /// Vertical spacing between the field's label and the input box.
    pub const LABEL_TO_FIELD_SPACING: CGFloat = 6.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_radius_matches_swift_text_field() {
        // ShadcnTextField uses cornerRadius: 8.
        assert_eq!(TextFieldMetrics::CORNER_RADIUS, 8.0);
    }

    #[test]
    fn field_height_matches_swift_text_field() {
        // ShadcnTextField uses .frame(height: 36).
        assert_eq!(TextFieldMetrics::FIELD_HEIGHT, 36.0);
    }
}
