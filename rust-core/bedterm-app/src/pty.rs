//! Canonical PTY dimension type.
//!
//! Defines [`PtyDimensions`], the canonical representation of terminal
//! viewport dimensions in character cells, plus optional pixel hints for
//! proper full-screen application layout over SSH.
//!
//! Both [`Serialize`](serde::Serialize) and [`Deserialize`](serde::Deserialize)
//! are derived so this type serves as the canonical JSON schema for the FFI
//! boundary alongside [`HostCredential`](crate::credential::HostCredential) and
//! [`SshConnectionRequest`](crate::credential::SshConnectionRequest).
//!
//! This is a pure-logic type with no iOS dependencies — compiles on
//! macOS host for unit tests.

use serde::{Deserialize, Serialize};

/// Terminal PTY dimensions in character cells, plus optional pixel hints.
///
/// Mirrors the Swift `PTYDimensions` struct but also carries pixel
/// dimensions, which the SSH `pty-req` and `window-change` channel requests
/// pass to the remote side for proper full-screen app layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PtyDimensions {
    /// Number of character columns (width in cells).
    pub cols: u16,
    /// Number of character rows (height in cells).
    pub rows: u16,
    /// Pixel width of the terminal viewport (may be 0 if unknown).
    pub width_px: u16,
    /// Pixel height of the terminal viewport (may be 0 if unknown).
    pub height_px: u16,
}

impl PtyDimensions {
    /// Convenience constructor from cols/rows only (pixel hints default to 0).
    pub const fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            width_px: 0,
            height_px: 0,
        }
    }

    /// Full constructor including pixel dimensions.
    pub const fn with_pixels(cols: u16, rows: u16, width_px: u16, height_px: u16) -> Self {
        Self {
            cols,
            rows,
            width_px,
            height_px,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that `PtyDimensions` derives Debug, Clone, Copy, PartialEq,
    /// Eq — needed by the trait methods and test assertions.
    #[test]
    fn pty_dimensions_derive() {
        let a = PtyDimensions::new(80, 24);
        let b = PtyDimensions::with_pixels(80, 24, 402, 874);
        let c = a;
        assert_eq!(a, c);
        assert_ne!(a, b);
        assert_eq!(b.cols, 80);
        assert_eq!(b.rows, 24);
        assert_eq!(b.width_px, 402);
        assert_eq!(b.height_px, 874);
    }

    /// Verifies that Copy semantics work (copy is bitwise, no drop).
    #[test]
    fn pty_dimensions_copy() {
        let dims = PtyDimensions::new(132, 43);
        let copied = dims; // Copy, not move
        assert_eq!(dims, copied);
    }

    /// Verifies that the default pixel dimensions are zero.
    #[test]
    fn pty_dimensions_default_pixels() {
        let dims = PtyDimensions::new(80, 24);
        assert_eq!(dims.width_px, 0);
        assert_eq!(dims.height_px, 0);
    }

    /// Verifies that `with_pixels` sets all four fields correctly.
    #[test]
    fn pty_dimensions_with_pixels_full() {
        let dims = PtyDimensions::with_pixels(100, 50, 800, 600);
        assert_eq!(dims.cols, 100);
        assert_eq!(dims.rows, 50);
        assert_eq!(dims.width_px, 800);
        assert_eq!(dims.height_px, 600);
    }

    /// Verifies that `PtyDimensions` is a `Copy` type (no heap-allocated fields).
    #[test]
    fn pty_dimensions_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<PtyDimensions>();
    }

    // -----------------------------------------------------------------------
    // Serialization / Deserialization
    // -----------------------------------------------------------------------

    /// Verify that `PtyDimensions` round-trips through JSON without loss.
    #[test]
    fn pty_dimensions_json_roundtrip() {
        let original = PtyDimensions::with_pixels(80, 24, 402, 874);
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: PtyDimensions = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    /// Verify JSON field names match the canonical schema.
    #[test]
    fn pty_dimensions_json_structure() {
        let dims = PtyDimensions::with_pixels(80, 24, 402, 874);
        let json = serde_json::to_string(&dims).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["cols"], 80);
        assert_eq!(parsed["rows"], 24);
        assert_eq!(parsed["width_px"], 402);
        assert_eq!(parsed["height_px"], 874);
    }

    /// Verify that `new()` constructor serialises pixel dimensions as zero.
    #[test]
    fn pty_dimensions_new_serializes_zero_pixels() {
        let dims = PtyDimensions::new(132, 43);
        let json = serde_json::to_string(&dims).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["width_px"], 0);
        assert_eq!(parsed["height_px"], 0);
    }

    // ── Edge cases ────────────────────────────────────────────────────

    /// Zero cols/rows is explicitly allowed (the type is `u16`, not
    /// `NonZeroU16`). Both constructors produce the same value at zero.
    #[test]
    fn pty_dimensions_zero_values() {
        let d = PtyDimensions::new(0, 0);
        assert_eq!(d.cols, 0);
        assert_eq!(d.rows, 0);
        assert_eq!(d.width_px, 0);
        assert_eq!(d.height_px, 0);

        let d = PtyDimensions::with_pixels(0, 0, 0, 0);
        assert_eq!(d, PtyDimensions::new(0, 0));
    }

    /// Maximum u16 values exercise the type's upper bound.
    #[test]
    fn pty_dimensions_max_values() {
        let d = PtyDimensions::new(u16::MAX, u16::MAX);
        assert_eq!(d.cols, u16::MAX);
        assert_eq!(d.rows, u16::MAX);
        assert_eq!(d.width_px, 0);
        assert_eq!(d.height_px, 0);

        let d = PtyDimensions::with_pixels(u16::MAX, u16::MAX, u16::MAX, u16::MAX);
        assert_eq!(d.cols, u16::MAX);
        assert_eq!(d.rows, u16::MAX);
        assert_eq!(d.width_px, u16::MAX);
        assert_eq!(d.height_px, u16::MAX);
    }

    /// `new` and `with_pixels` produce equal values when pixels are zero.
    #[test]
    fn pty_dimensions_constructors_equivalent() {
        assert_eq!(
            PtyDimensions::new(80, 24),
            PtyDimensions::with_pixels(80, 24, 0, 0)
        );
        assert_eq!(
            PtyDimensions::new(u16::MAX, u16::MAX),
            PtyDimensions::with_pixels(u16::MAX, u16::MAX, 0, 0)
        );
    }

    /// Same cols/rows but different pixel hints are not equal.
    #[test]
    fn pty_dimensions_pixels_affect_equality() {
        assert_ne!(
            PtyDimensions::new(80, 24),
            PtyDimensions::with_pixels(80, 24, 402, 874)
        );
    }

    /// Serialization round-trip with zero values.
    #[test]
    fn pty_dimensions_serialize_zero() {
        let d = PtyDimensions::new(0, 0);
        let json = serde_json::to_string(&d).unwrap();
        let deserialized: PtyDimensions = serde_json::from_str(&json).unwrap();
        assert_eq!(d, deserialized);
    }

    /// Serialization round-trip with max u16 values.
    #[test]
    fn pty_dimensions_serialize_max() {
        let d = PtyDimensions::with_pixels(u16::MAX, u16::MAX, u16::MAX, u16::MAX);
        let json = serde_json::to_string(&d).unwrap();
        let deserialized: PtyDimensions = serde_json::from_str(&json).unwrap();
        assert_eq!(d, deserialized);
    }
}
