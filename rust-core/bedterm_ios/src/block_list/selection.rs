//! Port of `BlockListSelectionController.swift` + the helper bridge
//! from `BlockListContainerView+Selection.swift`.
//!
//! Long-press → drag → copy state machine. Two pieces:
//!
//!   * `BlockSelectionState` — pure value state (active selection,
//!     metrics). Tested without UIKit.
//!   * `copy_to_pasteboard` — shoves a string into
//!     `UIPasteboard.generalPasteboard`. Mirrors Swift's exact format:
//!     **just the selected text**, no command-prefix or agent emoji.
//!     Multi-block selection is out of scope (v1).
//!
//! The actual long-press gesture is installed on the VC's scroll view
//! (see `mod.rs`) and its selector calls into `BlockSelectionState`.

#![allow(dead_code)]

use crate::geometry::CGFloat;
use crate::metal_selection_layer::SelectionRange;
// `copy_to_pasteboard` is the only UIKit-coupled function in this module;
// gated so the pure `BlockSelectionState` state machine stays host-testable.
#[cfg(target_os = "ios")]
use objc2_foundation::NSString;
#[cfg(target_os = "ios")]
use objc2_ui_kit::UIPasteboard;

/// Result of `block_hit_test` — passed back to begin/extend.
#[derive(Clone, Copy, Debug)]
pub struct BlockHit {
    pub block_id: u64,
    pub body_top: CGFloat,
    pub rows: i32,
    pub cols: i32,
}

/// Active long-press selection inside a single block.
#[derive(Clone, Copy, Debug)]
pub struct ActiveSelection {
    pub block_id: u64,
    pub body_top_in_content: CGFloat,
    pub cols: i32,
    pub rows: i32,
    pub range: SelectionRange,
}

/// Selection-state machine. Held by the block list VC.
#[derive(Default)]
pub struct BlockSelectionState {
    pub active: Option<ActiveSelection>,
    pub row_height_pt: CGFloat,
    pub cell_width_pt: CGFloat,
    pub container_width_pt: CGFloat,
    pub left_inset_pt: CGFloat,
}

impl BlockSelectionState {
    /// Update metrics from a layout pass — driven by the container.
    pub fn update_metrics(
        &mut self,
        cell_width_pt: CGFloat,
        row_height_pt: CGFloat,
        container_width_pt: CGFloat,
        left_inset_pt: CGFloat,
    ) {
        self.cell_width_pt = cell_width_pt;
        self.row_height_pt = row_height_pt;
        self.container_width_pt = container_width_pt;
        self.left_inset_pt = left_inset_pt;
    }

    fn clamp_row(&self, y_pt: CGFloat, rows: i32) -> i32 {
        if self.row_height_pt <= 0.0 {
            return 0;
        }
        ((y_pt / self.row_height_pt).floor() as i32).clamp(0, (rows - 1).max(0))
    }

    fn clamp_col(&self, x_pt: CGFloat, cols: i32) -> i32 {
        if self.cell_width_pt <= 0.0 {
            return 0;
        }
        ((x_pt / self.cell_width_pt).floor() as i32).clamp(0, cols.max(0))
    }

    pub fn begin(&mut self, point_in_content: (CGFloat, CGFloat), hit: BlockHit) {
        let (x, y) = point_in_content;
        let row = self.clamp_row(y - hit.body_top, hit.rows);
        let col = self.clamp_col(x - self.left_inset_pt, hit.cols);
        self.active = Some(ActiveSelection {
            block_id: hit.block_id,
            body_top_in_content: hit.body_top,
            cols: hit.cols,
            rows: hit.rows,
            range: SelectionRange {
                start_row: row,
                start_col: col,
                end_row: row,
                end_col: col + 1,
            },
        });
    }

    pub fn extend(&mut self, point_in_content: (CGFloat, CGFloat)) {
        let Some(sel) = self.active.as_mut() else {
            return;
        };
        let (x, y) = point_in_content;
        let row = ((y - sel.body_top_in_content) / self.row_height_pt.max(1.0))
            .floor()
            .clamp(0.0, (sel.rows - 1).max(0) as f64) as i32;
        let col = ((x - self.left_inset_pt) / self.cell_width_pt.max(1.0))
            .floor()
            .clamp(0.0, sel.cols.max(0) as f64) as i32;
        sel.range.end_row = row;
        sel.range.end_col = col + 1;
    }

    pub fn cancel(&mut self) {
        self.active = None;
    }

    /// Finish a drag: returns the (block_id, range) tuple to feed into
    /// the host's text extractor; the caller then copies the result.
    pub fn finish(&mut self) -> Option<(u64, SelectionRange)> {
        let out = self.active.map(|s| (s.block_id, s.range));
        self.active = None;
        out
    }
}

/// Copy a string to `UIPasteboard.generalPasteboard`. Mirrors Swift's
/// one-liner: just the selected text, nothing prepended.
#[cfg(target_os = "ios")]
pub fn copy_to_pasteboard(text: &str) {
    if text.is_empty() {
        return;
    }
    let s = NSString::from_str(text);
    let pb = UIPasteboard::generalPasteboard();
    unsafe { pb.setString(Some(&s)) };
}

/// Helper used by the long-press selector — walks block ranges and
/// returns the body-relative hit. Mirrors the Swift `blockHitTest`.
pub fn block_hit_test(
    ranges: &[super::layout::BlockRange],
    point_in_content: (CGFloat, CGFloat),
    header_height_pt: CGFloat,
    row_height_pt: CGFloat,
    cols: i32,
) -> Option<BlockHit> {
    let (_, y) = point_in_content;
    for r in ranges {
        let body_top = r.top + header_height_pt;
        let body_bot = r.bot;
        if y >= body_top && y < body_bot {
            let rows = r.block.body_rows as i32;
            // `cols` is the live grid width — caller passes it from the
            // container's cached metrics. We don't snapshot here (the
            // session-backed snapshot lands in W6).
            let _ = row_height_pt;
            return Some(BlockHit {
                block_id: r.block.id,
                body_top,
                rows,
                cols,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_creates_one_cell_selection() {
        let mut s = BlockSelectionState::default();
        s.update_metrics(9.0, 18.0, 400.0, 12.0);
        let hit = BlockHit {
            block_id: 7,
            body_top: 56.0,
            rows: 10,
            cols: 40,
        };
        s.begin((30.0, 60.0), hit);
        let a = s.active.unwrap();
        assert_eq!(a.block_id, 7);
        assert_eq!(a.range.start_row, 0);
        assert_eq!(a.range.start_col, 2);
        assert_eq!(a.range.end_col, 3);
    }

    #[test]
    fn finish_clears_state() {
        let mut s = BlockSelectionState::default();
        s.update_metrics(9.0, 18.0, 400.0, 12.0);
        s.begin(
            (30.0, 60.0),
            BlockHit {
                block_id: 1,
                body_top: 56.0,
                rows: 5,
                cols: 20,
            },
        );
        assert!(s.finish().is_some());
        assert!(s.active.is_none());
    }
}
