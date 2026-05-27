//! Port of `BlockPanelStyle.swift`.
//!
//! Tunables for the block list's layout. The Warp-style rounded panel chrome
//! was dropped in favour of a divider-separated list — blocks share the
//! scroll background; only a hairline `ShadcnBorder` line sits in the gap
//! between them.

#![allow(dead_code)]

/// Vertical gap between adjacent blocks. Blocks now butt against each other;
/// the hairline divider above each header is the only thing separating one
/// block from the next.
pub const INTER_BLOCK_GAP_PT: f32 = 0.0;

/// Horizontal inset between the panel's left edge and cell column 0.
pub const CELL_LEFT_INSET_PT: f32 = 12.0;

/// Hairline thickness drawn in the gap between blocks.
pub const DIVIDER_THICKNESS_PT: f32 = 1.0;

/// Horizontal inset for the divider hairline — slightly indented from the
/// screen edge so it reads as a list separator rather than a hard rule.
pub const DIVIDER_HORIZONTAL_INSET_PT: f32 = 16.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_swift_values() {
        assert_eq!(INTER_BLOCK_GAP_PT, 0.0);
        assert_eq!(CELL_LEFT_INSET_PT, 12.0);
        assert_eq!(DIVIDER_THICKNESS_PT, 1.0);
        assert_eq!(DIVIDER_HORIZONTAL_INSET_PT, 16.0);
    }
}
