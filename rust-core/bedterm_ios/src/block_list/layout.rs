//! Port of `BlockListContainerView+Layout.swift`.
//!
//! Pure value-type math: given a list of `BlockSnapshot`s plus the
//! per-row + per-header heights, produce one `BlockRange` per block in
//! scroll-content (point) coordinates. The metal renderer consumes the
//! same ranges in W6/W7 via an FFI descriptor table — we don't build
//! the FFI struct here because the live `BtBlockLayoutEntry` /
//! `BtBlockHeaderEntry` C structs aren't yet exposed through
//! `bedterm_core` to this crate (they live in BedTermIOS headers).
//! W7 lands the BlockSource → session glue, but the
//! `BtBlockLayoutEntry` / `BtBlockHeaderEntry` C bridge is still
//! pending — tracked as `TODO(post-block-list-painter-ffi)` in `block_list/mod.rs`.
//! Layout itself stays pure Rust here.

#![allow(dead_code)]

use super::source::BlockSnapshot;
use crate::block_panel_style::INTER_BLOCK_GAP_PT;
use crate::geometry::CGFloat;

/// Default header band height in points. Mirrors Swift's
/// `BlockListContainerViewController.headerHeightPt`.
pub const HEADER_HEIGHT_PT: CGFloat = 56.0;

/// One block's natural-flow extent in scroll-content space.
#[derive(Clone, Debug)]
pub struct BlockRange {
    pub block: BlockSnapshot,
    pub top: CGFloat,
    pub bot: CGFloat,
}

/// Body (cells region) height in points for the given block snapshot.
pub fn body_height_pt(block: &BlockSnapshot, row_height_pt: CGFloat) -> CGFloat {
    f64::from(block.body_rows) * row_height_pt
}

/// Walk the block list once, producing top/bottom Y per block.
/// Mirrors `computeBlockRanges` in the Swift extension.
pub fn compute_block_ranges(
    blocks: &[BlockSnapshot],
    header_height_pt: CGFloat,
    row_height_pt: CGFloat,
) -> Vec<BlockRange> {
    let gap = f64::from(INTER_BLOCK_GAP_PT);
    let mut out: Vec<BlockRange> = Vec::with_capacity(blocks.len());
    let mut y_pt: CGFloat = 0.0;
    for block in blocks {
        let top = y_pt;
        let bot = top + header_height_pt + body_height_pt(block, row_height_pt);
        out.push(BlockRange {
            block: block.clone(),
            top,
            bot,
        });
        y_pt = bot + gap;
    }
    out
}

/// Sum of all block extents (used to size the scroll content).
pub fn content_height_pt(
    blocks: &[BlockSnapshot],
    header_height_pt: CGFloat,
    row_height_pt: CGFloat,
) -> CGFloat {
    let gap = f64::from(INTER_BLOCK_GAP_PT);
    let mut total: CGFloat = 0.0;
    for block in blocks {
        total += header_height_pt + body_height_pt(block, row_height_pt) + gap;
    }
    total
}

#[cfg(test)]
mod tests {
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
    fn empty_list_zero_height() {
        assert_eq!(content_height_pt(&[], HEADER_HEIGHT_PT, 18.0), 0.0);
    }

    #[test]
    fn single_block_sums_header_plus_body() {
        let h = content_height_pt(&[snap(1, 10)], HEADER_HEIGHT_PT, 18.0);
        assert!((h - (HEADER_HEIGHT_PT + 10.0 * 18.0)).abs() < 0.001);
    }

    #[test]
    fn ranges_are_contiguous_with_gap() {
        let blocks = vec![snap(1, 4), snap(2, 3)];
        let r = compute_block_ranges(&blocks, HEADER_HEIGHT_PT, 18.0);
        assert_eq!(r.len(), 2);
        assert!((r[0].top - 0.0).abs() < 0.001);
        let expected_bot_0 = HEADER_HEIGHT_PT + 4.0 * 18.0;
        assert!((r[0].bot - expected_bot_0).abs() < 0.001);
        let gap = f64::from(INTER_BLOCK_GAP_PT);
        assert!((r[1].top - (expected_bot_0 + gap)).abs() < 0.001);
    }
}
