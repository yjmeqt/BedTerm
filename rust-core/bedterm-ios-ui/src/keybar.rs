//! Keybar accessory view for view2 — SwiftUI shadcn style.
//!
//! A horizontal row of "icon · label" chips matching the Swift
//! `BlockListComposer` footer: SF Symbol (size 11, weight medium) +
//! verbatim 12 pt label, muted-gray foreground, 8 h / 4 v padding,
//! no fill. Attached as `view2.inputAccessoryView` so it tracks the
//! keyboard above view2.

use crate::coordinator::BtIosKeyboardCoordinator;
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, msg_send_id, sel, ClassType};
use objc2_foundation::{MainThreadMarker, NSAttributedString, NSDictionary, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIButton, UIButtonConfiguration, UIColor, UIFont, UIImage,
    UIImageSymbolConfiguration, UIImageSymbolWeight, UILayoutConstraintAxis, UIStackView,
    UIStackViewAlignment, UIStackViewDistribution, UIView,
};

const ICON_TAB: &str = "arrow.right.to.line.compact";
const ICON_NEWLINE: &str = "return";
const ICON_ESC: &str = "escape";
const ICON_CTRL: &str = "control";

const FONT_SIZE: CGFloat = 12.0;
const SYMBOL_SIZE: CGFloat = 11.0;
const PADDING_H: CGFloat = 8.0;
const PADDING_V: CGFloat = 4.0;
const IMAGE_PADDING: CGFloat = 4.0;
pub(crate) const BAR_HEIGHT: CGFloat = 38.0;

/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;
/// `UIViewAutoresizingFlexibleWidth` = 1 << 1.
const AUTORESIZE_FLEXIBLE_W: u64 = 1 << 1;
/// `UIViewAutoresizingFlexibleHeight` = 1 << 4.
const AUTORESIZE_FLEXIBLE_H: u64 = 1 << 4;

pub(crate) fn make_keybar(
    mtm: MainThreadMarker,
    coordinator: &BtIosKeyboardCoordinator,
) -> Retained<UIView> {
    let bar_frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 320.0,
            height: BAR_HEIGHT,
        },
    };
    let wrapper: Retained<UIView> =
        unsafe { msg_send_id![mtm.alloc::<UIView>(), initWithFrame: bar_frame] };
    let _: () = unsafe { msg_send![&*wrapper, setAutoresizingMask: AUTORESIZE_FLEXIBLE_W] };

    let bg: Retained<UIColor> =
        unsafe { msg_send_id![UIColor::class(), secondarySystemBackgroundColor] };
    let _: () = unsafe { msg_send![&*wrapper, setBackgroundColor: &*bg] };

    let stack: Retained<UIStackView> =
        unsafe { msg_send_id![mtm.alloc::<UIStackView>(), initWithFrame: bar_frame] };
    unsafe {
        stack.setAxis(UILayoutConstraintAxis::Horizontal);
        stack.setAlignment(UIStackViewAlignment::Center);
        stack.setDistribution(UIStackViewDistribution::Fill);
        stack.setSpacing(0.0);
    }
    let _: () = unsafe {
        msg_send![&*stack, setAutoresizingMask: AUTORESIZE_FLEXIBLE_W | AUTORESIZE_FLEXIBLE_H]
    };
    let _: () = unsafe { msg_send![&*wrapper, addSubview: &*stack] };

    let target: &AnyObject = coordinator.as_ref() as &AnyObject;

    let muted = muted_foreground();
    let tab = make_chip(mtm, ICON_TAB, "Tab", &muted, target, sel!(keybarTab));
    let newline = make_chip(
        mtm,
        ICON_NEWLINE,
        "Newline",
        &muted,
        target,
        sel!(keybarNewline),
    );
    let esc = make_chip(mtm, ICON_ESC, "Esc", &muted, target, sel!(keybarEsc));
    let ctrl = make_chip(mtm, ICON_CTRL, "Ctrl", &muted, target, sel!(keybarCtrl));

    unsafe {
        stack.addArrangedSubview(&tab);
        stack.addArrangedSubview(&newline);
        stack.addArrangedSubview(&esc);
        stack.addArrangedSubview(&ctrl);
    }

    // Trailing spacer view so chips left-align under .fill distribution.
    let spacer_frame = CGRect::default();
    let spacer: Retained<UIView> =
        unsafe { msg_send_id![mtm.alloc::<UIView>(), initWithFrame: spacer_frame] };
    unsafe { stack.addArrangedSubview(&spacer) };

    coordinator.set_ctrl_button(&ctrl);

    wrapper
}

fn make_chip(
    mtm: MainThreadMarker,
    icon: &str,
    label: &str,
    foreground: &UIColor,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIButton> {
    let cfg = unsafe { UIButtonConfiguration::plainButtonConfiguration(mtm) };

    let title_ns = NSString::from_str(label);
    let font: Retained<UIFont> =
        unsafe { msg_send_id![UIFont::class(), systemFontOfSize: FONT_SIZE] };
    let font_attr_key = NSString::from_str("NSFont");
    let attrs: Retained<NSDictionary<NSString, AnyObject>> = unsafe {
        NSDictionary::from_vec(
            &[&*font_attr_key],
            vec![Retained::cast::<AnyObject>(font.clone())],
        )
    };
    let attributed: Retained<NSAttributedString> = unsafe {
        NSAttributedString::initWithString_attributes(
            mtm.alloc::<NSAttributedString>(),
            &title_ns,
            Some(&attrs),
        )
    };
    unsafe { cfg.setAttributedTitle(Some(&attributed)) };

    let icon_ns = NSString::from_str(icon);
    let sym_cfg = unsafe {
        UIImageSymbolConfiguration::configurationWithPointSize_weight(
            SYMBOL_SIZE,
            UIImageSymbolWeight::Medium,
        )
    };
    if let Some(image) = unsafe { UIImage::systemImageNamed(&icon_ns) } {
        let configured: Retained<UIImage> =
            unsafe { msg_send_id![&*image, imageByApplyingSymbolConfiguration: &*sym_cfg] };
        unsafe { cfg.setImage(Some(&configured)) };
    }
    unsafe { cfg.setImagePadding(IMAGE_PADDING) };
    let insets = NSDirectionalEdgeInsets {
        top: PADDING_V,
        leading: PADDING_H,
        bottom: PADDING_V,
        trailing: PADDING_H,
    };
    unsafe { cfg.setContentInsets(insets) };
    unsafe { cfg.setBaseForegroundColor(Some(foreground)) };

    // `buttonWithType: 0` = `.custom`; setConfiguration overrides visuals.
    let button: Retained<UIButton> =
        unsafe { msg_send_id![UIButton::class(), buttonWithType: 0_i64] };
    unsafe { button.setConfiguration(Some(&cfg)) };
    let _: () = unsafe {
        msg_send![&*button,
            addTarget: target,
            action: action,
            forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE,
        ]
    };
    button
}

/// Approximate `Color("ShadcnMutedForeground")` — system `secondaryLabel` is
/// adaptive (lighter on dark, darker on light) and visually matches the
/// shadcn token (0.451 light / 0.631 dark) closely enough.
fn muted_foreground() -> Retained<UIColor> {
    unsafe { msg_send_id![UIColor::class(), secondaryLabelColor] }
}
