//! Port of `MetalSelectionLayer.swift` + `MetalSelectionGesture.swift`.
//!
//! Two things in one module:
//!   * `SelectionRange` — row/col interval, plus a `rects()` helper that
//!     produces one CGRect per covered row (matching Swift parity).
//!   * `MetalSelectionLayer` — `CAShapeLayer` overlay rendering the union of
//!     those rects under a translucent system-blue fill.
//!
//! The long-press gesture lives on `BtIosMetalInputView` directly (see
//! `metal_view.rs`) so it can route taps through the existing selector /
//! coordinator plumbing without an extra glue type.

#![allow(dead_code)]

use bedterm_app::geometry::{CGPoint, CGRect, CGSize};

// UIKit + QuartzCore bindings only exist on iOS — the pure
// `SelectionRange` math below stays host-compilable, the
// `MetalSelectionLayer` wrapper that talks to `CAShapeLayer` /
// `UIBezierPath` is gated.
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::ClassType;
use objc2_quartz_core::CAShapeLayer;
use objc2_ui_kit::{UIBezierPath, UIColor};

// Re-export pure SelectionRange from bedterm-app
pub use bedterm_app::selection_range::SelectionRange;

/// `CAShapeLayer` overlay rendering the union of `SelectionRange::rects()`.
pub struct MetalSelectionLayer {
    layer: Retained<CAShapeLayer>,
}

impl MetalSelectionLayer {
    pub fn new() -> Self {
        let layer: Retained<CAShapeLayer> = unsafe { msg_send![CAShapeLayer::class(), layer] };
        let this = Self { layer };
        this.apply_appearance();
        this
    }

    pub fn layer(&self) -> &CAShapeLayer {
        &self.layer
    }

    pub fn set_hidden(&self, hidden: bool) {
        unsafe {
            let _: () = msg_send![&*self.layer, setHidden: hidden];
        }
    }

    /// Re-resolve the fill colour from the system blue token. Mirror Swift's
    /// `UIColor.systemBlue.withAlphaComponent(0.35)`.
    pub fn apply_appearance(&self) {
        let blue: Retained<UIColor> = unsafe { msg_send![UIColor::class(), systemBlueColor] };
        let translucent: Retained<UIColor> =
            unsafe { msg_send![&*blue, colorWithAlphaComponent: 0.35_f64] };
        let cg_fill: *const AnyObject = unsafe { msg_send![&*translucent, CGColor] };
        let clear: Retained<UIColor> = unsafe { msg_send![UIColor::class(), clearColor] };
        let cg_clear: *const AnyObject = unsafe { msg_send![&*clear, CGColor] };
        unsafe {
            let _: () = msg_send![&*self.layer, setFillColor: cg_fill];
            let _: () = msg_send![&*self.layer, setStrokeColor: cg_clear];
        }
    }

    /// Update the layer's path. `None` clears it.
    pub fn update(&self, range: Option<SelectionRange>, cell_size: CGSize, cols: i32) {
        let Some(range) = range else {
            unsafe {
                let _: () = msg_send![&*self.layer, setPath: std::ptr::null::<AnyObject>()];
            }
            return;
        };
        let combined: Retained<UIBezierPath> = UIBezierPath::bezierPath();
        for rect in range.rects(cell_size, cols) {
            // Use msg_send! to avoid the typed-wrapper's foundation-CGRect arg.
            let r: Retained<UIBezierPath> =
                unsafe { msg_send![UIBezierPath::class(), bezierPathWithRect: rect] };
            combined.appendPath(&r);
        }
        let cg_path: *const AnyObject = unsafe { msg_send![&*combined, CGPath] };
        unsafe {
            let _: () = msg_send![&*self.layer, setPath: cg_path];
        }
    }
}

impl Default for MetalSelectionLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_empty_selection_yields_no_rects() {
        let r = SelectionRange {
            start_row: 2,
            start_col: 5,
            end_row: 2,
            end_col: 5,
        };
        assert!(r
            .rects(
                CGSize {
                    width: 8.0,
                    height: 16.0
                },
                80
            )
            .is_empty());
    }

    #[test]
    fn single_row_selection_yields_one_rect() {
        let r = SelectionRange {
            start_row: 1,
            start_col: 2,
            end_row: 1,
            end_col: 5,
        };
        let rects = r.rects(
            CGSize {
                width: 8.0,
                height: 16.0,
            },
            80,
        );
        assert_eq!(rects.len(), 1);
        assert!((rects[0].origin.x - 16.0).abs() < 0.001);
        assert!((rects[0].size.width - 24.0).abs() < 0.001);
    }

    #[test]
    fn multi_row_selection_one_rect_per_row() {
        let r = SelectionRange {
            start_row: 0,
            start_col: 3,
            end_row: 2,
            end_col: 4,
        };
        let rects = r.rects(
            CGSize {
                width: 8.0,
                height: 16.0,
            },
            80,
        );
        assert_eq!(rects.len(), 3);
    }

    #[test]
    fn reverse_selection_is_normalised() {
        let r = SelectionRange {
            start_row: 5,
            start_col: 10,
            end_row: 2,
            end_col: 4,
        };
        let n = r.normalised();
        assert_eq!(n.start_row, 2);
        assert_eq!(n.end_row, 5);
    }
}
