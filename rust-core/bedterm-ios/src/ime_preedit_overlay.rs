//! Port of `IMEPreeditOverlay.swift`.
//!
//! A floating `UIView` shown near the cursor while the IME is composing
//! (Pinyin, Japanese Romaji, Korean 2-set, etc.). Hosted by
//! `BtIosMetalInputView` and driven from the marked-text path. Visual parity
//! with the Swift overlay: rounded 4 pt corners, 1 pt border in the
//! `ShadcnBorder` token, `ShadcnBackground` fill, `ShadcnPrimary` label.

#![allow(dead_code)]

use crate::design_system::colors;
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::NSString;
use objc2_ui_kit::{UIFont, UIFontWeight, UILabel, UIView};
use std::cell::RefCell;

const INSET_X: CGFloat = 6.0;
const INSET_Y: CGFloat = 2.0;

#[derive(Default)]
pub struct Ivars {
    label: RefCell<Option<Retained<UILabel>>>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// Floating overlay rendering the current IME pre-edit string. Subclasses
    /// `UIView`. Holds a single `UILabel` inset by 6 pt × 2 pt.
    #[unsafe(super(UIView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosImePreeditOverlay"]
    #[ivars = Ivars]
    pub struct ImePreeditOverlay;

    impl ImePreeditOverlay {
        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(this: Allocated<Self>, frame: CGRect) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
            Some(this)
        }

        /// Lay out the inner label inset from `bounds`.
        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), layoutSubviews] };
            let label_ref = self.ivars().label.borrow();
            let Some(label) = label_ref.as_ref() else {
                return;
            };
            let bounds: CGRect = unsafe { msg_send![self, bounds] };
            let inner = CGRect {
                origin: CGPoint {
                    x: bounds.origin.x + INSET_X,
                    y: bounds.origin.y + INSET_Y,
                },
                size: CGSize {
                    width: (bounds.size.width - 2.0 * INSET_X).max(0.0),
                    height: (bounds.size.height - 2.0 * INSET_Y).max(0.0),
                },
            };
            let _: () = unsafe { msg_send![&**label, setFrame: inner] };
        }
    }
);

impl ImePreeditOverlay {
    /// Build a fresh overlay sized to zero. The host re-frames on every
    /// `update_text`.
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let zero = CGRect::default();
        let this: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), initWithFrame: zero] };
        unsafe {
            let _: () = msg_send![&*this, setUserInteractionEnabled: false];
        }
        // Layer chrome.
        let layer: Retained<AnyObject> = unsafe { msg_send![&*this, layer] };
        unsafe {
            let _: () = msg_send![&*layer, setCornerRadius: 4.0_f64];
            let _: () = msg_send![&*layer, setBorderWidth: 1.0_f64];
        }
        let border = colors::shadcn_border();
        let bg = colors::shadcn_background();
        let fg = colors::shadcn_primary();

        let cg_border: *const AnyObject = unsafe { msg_send![&*border, CGColor] };
        unsafe {
            let _: () = msg_send![&*layer, setBorderColor: cg_border];
            let _: () = msg_send![&*this, setBackgroundColor: &*bg];
        }

        // Embedded label.
        let label: Retained<UILabel> =
            unsafe { msg_send![UILabel::alloc(mtm), initWithFrame: zero] };
        unsafe {
            label.setTextColor(Some(&fg));
            let _: () = msg_send![&*label, setNumberOfLines: 1_i64];
            let _: () = msg_send![&*label, setAdjustsFontForContentSizeCategory: false];
            let _: () = msg_send![&*this, addSubview: &*label];
        }
        *this.ivars().label.borrow_mut() = Some(label);

        this
    }

    /// Update the preedit string and pick the font. `font_pt` is the
    /// monospaced point size; the overlay re-rasterises on every set.
    pub fn set_text(&self, text: &str, font_pt: f64) {
        let label_ref = self.ivars().label.borrow();
        let Some(label) = label_ref.as_ref() else {
            return;
        };
        let font: Retained<UIFont> =
            UIFont::monospacedSystemFontOfSize_weight(font_pt, 0.0 as UIFontWeight);
        unsafe {
            label.setFont(Some(&font));
        }
        let s = NSString::from_str(text);
        let _: () = unsafe { msg_send![&**label, setText: &*s] };
        let _: () = unsafe { msg_send![self, setNeedsLayout] };
    }

    /// Compute the natural size for the current label content + inset.
    pub fn size_that_fits(&self, max: CGSize) -> CGSize {
        let label_ref = self.ivars().label.borrow();
        let Some(label) = label_ref.as_ref() else {
            return CGSize::default();
        };
        let inner_max = CGSize {
            width: (max.width - 2.0 * INSET_X).max(0.0),
            height: (max.height - 2.0 * INSET_Y).max(0.0),
        };
        let text_size: CGSize = unsafe { msg_send![&**label, sizeThatFits: inner_max] };
        CGSize {
            width: text_size.width.ceil() + 2.0 * INSET_X,
            height: text_size.height.ceil() + 2.0 * INSET_Y,
        }
    }
}
