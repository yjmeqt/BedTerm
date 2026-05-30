//! Direction pad — VT100 arrow-key emitter.
//!
//! Four chevron buttons arranged in a "+" layout. Each tap sends an
//! ESC `[`A/B/C/D byte sequence through the coordinator's `dpad*:`
//! selectors, which in turn route through `insertText:` on view2.
//!
//! Mirrors `DirectionPad.swift` in spirit — direction-only emission to
//! the four cursor-key VT100 escapes (see `KeyBarState.emitArrow`).
//! Long-press auto-repeat is deferred (see TODO below).

// The dpad isn't wired into the VC yet (the task spec restricts this wave to
// exporting `make_dpad` only). Suppress dead-code warnings — clippy is run
// with `-D warnings`.
#![allow(dead_code)]

use crate::coordinator::BtIosKeyboardCoordinator;
use bedterm_app::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, sel, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIBackgroundConfiguration, UIButton, UIButtonConfiguration, UIColor,
    UIImage, UIImageSymbolConfiguration, UIImageSymbolWeight, UIView,
};

/// Outer dpad container dimension (square). Exported so the VC can size /
/// position it in `viewDidLayoutSubviews`.
pub const DPAD_SIZE: CGFloat = 80.0;

/// Per-arrow button square dimension.
const BUTTON_SIZE: CGFloat = 24.0;

/// SF Symbol point size for the chevrons — matches the task spec (12 pt bold).
const SYMBOL_SIZE: CGFloat = 12.0;

/// Corner radius for each chevron button — small and tight; the dpad itself
/// is the visual cluster, not the individual chips.
const BUTTON_CORNER_RADIUS: CGFloat = 6.0;

/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;

/// Build the dpad UIView. The returned view is unparented; the caller
/// (typically `vc.rs`) is responsible for adding it as a subview and laying
/// it out via the exported `DPAD_SIZE` constant.
///
/// TODO: long-press auto-repeat. For now each tap emits exactly one arrow
/// sequence.
pub(crate) fn make_dpad(
    mtm: MainThreadMarker,
    coordinator: &BtIosKeyboardCoordinator,
) -> Retained<UIView> {
    let frame = CGRect {
        origin: CGPoint::default(),
        size: CGSize {
            width: DPAD_SIZE,
            height: DPAD_SIZE,
        },
    };
    let container: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: frame] };

    let target: &AnyObject = coordinator.as_ref() as &AnyObject;

    // Centre point of the dpad cluster.
    let centre = DPAD_SIZE / 2.0;
    let half_btn = BUTTON_SIZE / 2.0;
    // Push each chevron just past the centre so they form a "+".
    let offset = BUTTON_SIZE; // ≈ 24 pt from centre

    let up = make_arrow_button(mtm, "chevron.up", target, sel!(dpadUp:));
    place(&up, centre - half_btn, centre - half_btn - offset);
    let _: () = unsafe { msg_send![&*container, addSubview: &*up] };

    let down = make_arrow_button(mtm, "chevron.down", target, sel!(dpadDown:));
    place(&down, centre - half_btn, centre - half_btn + offset);
    let _: () = unsafe { msg_send![&*container, addSubview: &*down] };

    let left = make_arrow_button(mtm, "chevron.left", target, sel!(dpadLeft:));
    place(&left, centre - half_btn - offset, centre - half_btn);
    let _: () = unsafe { msg_send![&*container, addSubview: &*left] };

    let right = make_arrow_button(mtm, "chevron.right", target, sel!(dpadRight:));
    place(&right, centre - half_btn + offset, centre - half_btn);
    let _: () = unsafe { msg_send![&*container, addSubview: &*right] };

    container
}

fn place(button: &UIButton, x: CGFloat, y: CGFloat) {
    let frame = CGRect {
        origin: CGPoint { x, y },
        size: CGSize {
            width: BUTTON_SIZE,
            height: BUTTON_SIZE,
        },
    };
    let _: () = unsafe { msg_send![button, setFrame: frame] };
}

fn make_arrow_button(
    mtm: MainThreadMarker,
    icon: &str,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIButton> {
    let cfg = UIButtonConfiguration::plainButtonConfiguration(mtm);

    // 12 pt bold SF Symbol chevron.
    let icon_ns = NSString::from_str(icon);
    let sym_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
        SYMBOL_SIZE,
        UIImageSymbolWeight::Bold,
    );
    if let Some(image) = UIImage::systemImageNamed(&icon_ns) {
        let configured: Retained<UIImage> =
            unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*sym_cfg] };
        cfg.setImage(Some(&configured));
    }

    // Tight, square content insets so the chevron sits centred in a 24x24 cell.
    let insets = NSDirectionalEdgeInsets {
        top: 4.0,
        leading: 4.0,
        bottom: 4.0,
        trailing: 4.0,
    };
    cfg.setContentInsets(insets);

    // Faint background tile, rounded corners — visually similar to a keybar
    // chip but smaller and icon-only.
    let bg_cfg = UIBackgroundConfiguration::clearConfiguration(mtm);
    bg_cfg.setCornerRadius(BUTTON_CORNER_RADIUS);
    let bg_colour: Retained<UIColor> =
        unsafe { msg_send![UIColor::class(), secondarySystemFillColor] };
    bg_cfg.setBackgroundColor(Some(&bg_colour));
    cfg.setBackground(&bg_cfg);

    let fg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), secondaryLabelColor] };
    cfg.setBaseForegroundColor(Some(&fg));

    // `buttonWithType: 0` = `.custom`.
    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    let _: () = unsafe {
        msg_send![&*button,
            addTarget: target,
            action: action,
            forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE,
        ]
    };
    button
}
