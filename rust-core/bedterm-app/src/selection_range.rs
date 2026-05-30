//! Pure selection-range math — row/col interval with a `rects()` helper
//! that produces one CGRect per covered row.
//!
//! Extracted from `metal_selection_layer.rs`; the `MetalSelectionLayer`
//! CAShapeLayer wrapper stays in `bedterm-ios`.

#![allow(dead_code)]

use crate::geometry::{CGPoint, CGRect, CGSize};

/// Inclusive row/col selection. `normalised()` orders endpoints in row-major
/// order so `start ≤ end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRange {
    pub start_row: i32,
    pub start_col: i32,
    pub end_row: i32,
    pub end_col: i32,
}

impl SelectionRange {
    pub fn normalised(self) -> Self {
        let starts_first = (self.start_row < self.end_row)
            || (self.start_row == self.end_row && self.start_col <= self.end_col);
        if starts_first {
            self
        } else {
            Self {
                start_row: self.end_row,
                start_col: self.end_col,
                end_row: self.start_row,
                end_col: self.start_col,
            }
        }
    }

    /// One rect per row touched by the selection. Matches the Swift
    /// implementation byte-for-byte for parity tests in
    /// `SelectionGeometryTests`.
    pub fn rects(self, cell_size: CGSize, cols: i32) -> Vec<CGRect> {
        let norm = self.normalised();
        if norm.start_row == norm.end_row && norm.start_col == norm.end_col {
            return Vec::new();
        }
        let mut out = Vec::with_capacity((norm.end_row - norm.start_row + 1).max(0) as usize);
        for row in norm.start_row..=norm.end_row {
            let from = if row == norm.start_row {
                norm.start_col
            } else {
                0
            };
            let to = if row == norm.end_row {
                norm.end_col
            } else {
                cols
            };
            out.push(CGRect {
                origin: CGPoint {
                    x: f64::from(from) * cell_size.width,
                    y: f64::from(row) * cell_size.height,
                },
                size: CGSize {
                    width: f64::from(to - from) * cell_size.width,
                    height: cell_size.height,
                },
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalised_stable_when_already_ordered() {
        let r = SelectionRange {
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 5,
        };
        assert_eq!(r.normalised(), r);
    }

    #[test]
    fn normalised_swaps_when_reversed() {
        let r = SelectionRange {
            start_row: 1,
            start_col: 5,
            end_row: 0,
            end_col: 0,
        };
        let n = r.normalised();
        assert_eq!(n.start_row, 0);
        assert_eq!(n.start_col, 0);
        assert_eq!(n.end_row, 1);
        assert_eq!(n.end_col, 5);
    }

    #[test]
    fn rects_empty_when_no_selection() {
        let r = SelectionRange {
            start_row: 1,
            start_col: 5,
            end_row: 1,
            end_col: 5,
        };
        assert!(r.rects(CGSize::default(), 80).is_empty());
    }
}
