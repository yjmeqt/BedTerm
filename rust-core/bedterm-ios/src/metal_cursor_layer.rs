//! Port of `MetalCursorLayer.swift`.
//!
//! A `CALayer` sublayer that paints the terminal text cursor and blinks at
//! 530 ms half-cycle. Frame depends on the active `CursorStyle` (block / beam /
//! underline); colour is sourced from `terminal_palette::foreground()` so dark
//! / light flips pick up the right token.

#![allow(dead_code)]

use crate::terminal_palette;
use bedterm_app::geometry::{CGPoint, CGRect, CGSize};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSNumber, NSString};
use objc2_quartz_core::{CABasicAnimation, CALayer, CATransaction};
use objc2_ui_kit::UIColor;

/// Cursor visual style. Mirrors xterm's `\e[N q` codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorStyle {
    #[default]
    Block,
    Beam,
    Underline,
}

/// Thin owning wrapper around a `CALayer` configured as a terminal cursor.
pub struct MetalCursorLayer {
    layer: Retained<CALayer>,
    style: std::cell::Cell<CursorStyle>,
}

impl MetalCursorLayer {
    pub fn new() -> Self {
        let layer: Retained<CALayer> = CALayer::layer();
        let this = Self {
            layer,
            style: std::cell::Cell::new(CursorStyle::Block),
        };
        unsafe {
            let _: () = msg_send![&*this.layer, setCornerRadius: 1.0_f64];
        }
        this.apply_appearance();
        this.start_blink();
        this
    }

    /// Read-only access to the underlying `CALayer` so the host view can
    /// install it as a sublayer.
    pub fn layer(&self) -> &CALayer {
        &self.layer
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.layer.setHidden(hidden);
    }

    pub fn set_style(&self, style: CursorStyle) {
        self.style.set(style);
    }

    /// Place the cursor at `(col, row)` for a grid with cells of `cell_size`.
    /// Wrapped in a no-implicit-animation CATransaction so movement is
    /// instantaneous; the only animation is the blink.
    pub fn update(&self, col: i32, row: i32, cell_size: CGSize) {
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        let (x, y, w, h) = match self.style.get() {
            CursorStyle::Block => (
                f64::from(col) * cell_size.width,
                f64::from(row) * cell_size.height,
                cell_size.width,
                cell_size.height,
            ),
            CursorStyle::Beam => (
                f64::from(col) * cell_size.width,
                f64::from(row) * cell_size.height,
                (cell_size.width * 0.15).max(1.0),
                cell_size.height,
            ),
            CursorStyle::Underline => {
                let thick = (cell_size.height * 0.12).max(1.0);
                (
                    f64::from(col) * cell_size.width,
                    f64::from(row) * cell_size.height + cell_size.height - thick,
                    cell_size.width,
                    thick,
                )
            }
        };
        let new_frame = CGRect {
            origin: CGPoint { x, y },
            size: CGSize {
                width: w,
                height: h,
            },
        };
        unsafe {
            let _: () = msg_send![&*self.layer, setFrame: new_frame];
        }
        CATransaction::commit();
    }

    /// Re-resolve fill colour from `terminal_palette::foreground()`. Call from
    /// the host view's `applyAppearance()` so light/dark flips refresh the
    /// CGColor reference.
    pub fn apply_appearance(&self) {
        let fg = terminal_palette::foreground();
        let translucent: Retained<UIColor> =
            unsafe { msg_send![&*fg, colorWithAlphaComponent: 0.85_f64] };
        let cg: *const AnyObject = unsafe { msg_send![&*translucent, CGColor] };
        unsafe {
            let _: () = msg_send![&*self.layer, setBackgroundColor: cg];
        }
    }

    /// Install the 0.53 s autoreversing opacity blink under key "blink".
    pub fn start_blink(&self) {
        let key_path = NSString::from_str("opacity");
        let anim: Retained<CABasicAnimation> =
            CABasicAnimation::animationWithKeyPath(Some(&key_path));
        let one: Retained<NSNumber> = NSNumber::numberWithDouble(1.0_f64);
        let zero: Retained<NSNumber> = NSNumber::numberWithDouble(0.0_f64);
        unsafe {
            anim.setFromValue(Some(&*one));
            anim.setToValue(Some(&*zero));
            let _: () = msg_send![&*anim, setDuration: 0.53_f64];
            let _: () = msg_send![&*anim, setAutoreverses: true];
            let _: () = msg_send![&*anim, setRepeatCount: f32::INFINITY];
        }
        let key_name = NSString::from_str("blink");
        // Upcast CABasicAnimation -> CAAnimation for the typed method.
        let anim_super: &objc2_quartz_core::CAAnimation = unsafe {
            &*(&*anim as *const CABasicAnimation as *const objc2_quartz_core::CAAnimation)
        };
        self.layer.addAnimation_forKey(anim_super, Some(&key_name));
    }

    /// Stop the blink animation. Used when the cursor is hidden.
    pub fn stop_blink(&self) {
        let key_name = NSString::from_str("blink");
        self.layer.removeAnimationForKey(&key_name);
    }
}

impl Default for MetalCursorLayer {
    fn default() -> Self {
        Self::new()
    }
}
