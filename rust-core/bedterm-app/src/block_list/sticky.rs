//! Port of `BlockListContainerView+Sticky.swift`.
//!
//! Computes the section-style pinned header descriptor for whichever
//! block straddles the top of the viewport. Pure math + a borrow of the
//! block ranges produced by [`super::layout`].
//!
//! In the Swift impl this descriptor flows straight into the metal
//! renderer's `BtBlockHeaderEntry` array with `is_sticky = 1`. The FFI
//! struct lives behind a header that this crate doesn't yet bind (W7),
//! so for W4 the sticky path stops at the pinned-Y computation — the
//! parent VC will wire it to the renderer later.

#![allow(dead_code)]

use super::layout::BlockRange;
use crate::geometry::CGFloat;

/// Result of the sticky pass.
#[derive(Clone, Debug)]
pub struct StickyDescriptor {
    /// The block whose header is currently pinned.
    pub block_id: u64,
    /// Index into the `ranges` slice (caller already has the snapshot).
    pub index: usize,
    /// Pinned screen-space Y (origin of the band, in points).
    pub pinned_y_pt: CGFloat,
}

/// Index of the block whose header should be pinned, or `None` when no
/// block intersects the band.
pub fn sticky_active_index(ranges: &[BlockRange], scroll_y_pt: CGFloat) -> Option<usize> {
    if ranges.is_empty() {
        return None;
    }
    let mut found: Option<usize> = None;
    for (idx, range) in ranges.iter().enumerate() {
        if range.top <= scroll_y_pt && range.bot > scroll_y_pt {
            found = Some(idx);
        }
    }
    found
}

/// Build the pinned-header descriptor or `None` when no band is active.
pub fn build_sticky_descriptor(
    ranges: &[BlockRange],
    scroll_y_pt: CGFloat,
    header_height_pt: CGFloat,
) -> Option<StickyDescriptor> {
    let active = sticky_active_index(ranges, scroll_y_pt)?;
    let range = &ranges[active];
    let next_top = ranges
        .get(active + 1)
        .map(|r| r.top)
        .unwrap_or(f64::INFINITY);

    // Screen-space Y where the in-flow header would sit, then clamp at
    // 0 so it pins; push it back down once the next block's natural
    // header enters the band so the two hand off cleanly.
    let natural_screen_y = range.top - scroll_y_pt;
    let push_up_limit = (next_top - scroll_y_pt) - header_height_pt;
    let pinned_y = natural_screen_y.max(0.0).min(push_up_limit);

    Some(StickyDescriptor {
        block_id: range.block.id,
        index: active,
        pinned_y_pt: pinned_y,
    })
}

#[cfg(test)]
mod tests {
    use super::super::layout::{compute_block_ranges, HEADER_HEIGHT_PT};
    use super::super::source::BlockSnapshot;
    use super::*;

    fn snap(id: u64, body_rows: u32) -> BlockSnapshot {
        BlockSnapshot {
            id,
            command: String::new(),
            subtitle: None,
            body_rows,
            is_running: false,
            has_frozen_snapshot: false,
            agent: None,
            start_line: 0,
            end_line: None,
        }
    }

    #[test]
    fn no_blocks_no_sticky() {
        assert!(sticky_active_index(&[], 0.0).is_none());
    }

    #[test]
    fn first_block_pinned_when_scrolled_past_its_top() {
        let blocks = vec![snap(1, 8), snap(2, 8)];
        let ranges = compute_block_ranges(&blocks, HEADER_HEIGHT_PT, 18.0);
        // Scroll inside block 0's body — first block is the active sticky.
        let scroll_y = HEADER_HEIGHT_PT + 4.0 * 18.0;
        let idx = sticky_active_index(&ranges, scroll_y).unwrap();
        assert_eq!(idx, 0);
    }

    #[test]
    fn pinned_y_clamps_at_top_when_scrolled_within_block() {
        let blocks = vec![snap(1, 8), snap(2, 8)];
        let ranges = compute_block_ranges(&blocks, HEADER_HEIGHT_PT, 18.0);
        let scroll_y = HEADER_HEIGHT_PT + 2.0 * 18.0;
        let d = build_sticky_descriptor(&ranges, scroll_y, HEADER_HEIGHT_PT).unwrap();
        assert!((d.pinned_y_pt - 0.0).abs() < 0.001);
    }
}
