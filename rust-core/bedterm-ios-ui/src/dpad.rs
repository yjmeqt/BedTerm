//! Stub direction-pad placeholder.
//!
//! TODO: real dpad with arrow keys (emit ESC[A/B/C/D into the pipeline).
//! For now, this is purely visual — a fixed 80x32 UILabel container with
//! a `secondaryLabel` border.

use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{UIColor, UIFont, UILabel, UIView};

#[allow(dead_code)]
pub(crate) const DPAD_W: CGFloat = 80.0;
#[allow(dead_code)]
pub(crate) const DPAD_H: CGFloat = 32.0;

#[allow(dead_code)]
pub(crate) fn make_dpad_placeholder(mtm: MainThreadMarker) -> Retained<UIView> {
    let frame = CGRect {
        origin: CGPoint::default(),
        size: CGSize {
            width: DPAD_W,
            height: DPAD_H,
        },
    };
    let v: Retained<UIView> = unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: frame] };

    // Border: 1pt secondaryLabel, 4pt corner.
    let border: Retained<UIColor> = unsafe { msg_send![UIColor::class(), secondaryLabelColor] };
    let layer: Retained<AnyObject> = unsafe { msg_send![&*v, layer] };
    unsafe {
        let cg: *const AnyObject = msg_send![&*border, CGColor];
        let _: () = msg_send![&*layer, setBorderColor: cg];
        let _: () = msg_send![&*layer, setBorderWidth: 1.0_f64];
        let _: () = msg_send![&*layer, setCornerRadius: 4.0_f64];
        let _: () = msg_send![&*layer, setMasksToBounds: true];
    }

    let label: Retained<UILabel> =
        unsafe { msg_send![mtm.alloc::<UILabel>(), initWithFrame: v.bounds()] };
    let _: () = unsafe { msg_send![&*label, setAutoresizingMask: (1u64 << 1) | (1u64 << 4)] };
    let s = NSString::from_str("dpad");
    let _: () = unsafe { msg_send![&*label, setText: &*s] };
    let font: Retained<UIFont> = unsafe { msg_send![UIFont::class(), systemFontOfSize: 11.0_f64] };
    unsafe { label.setFont(Some(&font)) };
    let fg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), secondaryLabelColor] };
    unsafe { label.setTextColor(Some(&fg)) };
    let _: () = unsafe { msg_send![&*label, setTextAlignment: 1_i64] };
    let _: () = unsafe { msg_send![&*v, addSubview: &*label] };

    v
}
