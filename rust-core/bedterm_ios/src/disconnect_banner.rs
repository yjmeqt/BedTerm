//! Disconnect banner — UIKit port of `DisconnectBanner.swift`.
//!
//! Horizontal banner with a reason label on the leading edge and a
//! "Reconnect" primary button on the trailing edge. Background is the
//! `ShadcnDestructive` token (red) so the banner reads as an error
//! surface; the Swift original uses `.regularMaterial`, but the action
//! brief specifies the destructive token for the UIKit port.
//!
//! The factory returns a `UIView` whose frame the caller (the VC)
//! positions — autoresizing flags let it stretch with its parent's
//! width.

// The disconnect banner isn't wired into the VC yet (this wave only adds
// the factory). Suppress dead-code warnings — clippy is run with
// `-D warnings`.
#![allow(dead_code)]

use crate::design_system::colors;
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSAttributedString, NSDictionary, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIButton, UIButtonConfiguration, UIFont, UILabel,
    UILayoutConstraintAxis, UIStackView, UIStackViewAlignment, UIStackViewDistribution, UIView,
};

const PADDING_H: CGFloat = 12.0;
const PADDING_V: CGFloat = 8.0;
const SPACING: CGFloat = 8.0;
const BANNER_HEIGHT: CGFloat = 44.0;
const LABEL_FONT_SIZE: CGFloat = 16.0; // .callout ~ 16pt
const BUTTON_FONT_SIZE: CGFloat = 15.0;
const FONT_WEIGHT_SEMIBOLD: CGFloat = 0.3;
const CORNER_RADIUS: CGFloat = 6.0;
const LABEL_MAX_LINES: i64 = 2;

/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;
/// `UIViewAutoresizingFlexibleWidth` = 1 << 1.
const AUTORESIZE_FLEXIBLE_W: u64 = 1 << 1;

/// Build the disconnect banner.
///
/// - `mtm`: main-thread marker (UIKit construction).
/// - `message`: localised reason string shown on the leading edge.
/// - `target` / `action`: target/selector wiring for the Reconnect
///   button (typically the coordinator + `disconnectBannerReconnect:`).
pub(crate) fn make_disconnect_banner(
    mtm: MainThreadMarker,
    message: &str,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIView> {
    let banner_frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 320.0,
            height: BANNER_HEIGHT,
        },
    };
    let banner: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: banner_frame] };
    let _: () = unsafe { msg_send![&*banner, setAutoresizingMask: AUTORESIZE_FLEXIBLE_W] };

    let bg = colors::shadcn_destructive();
    let _: () = unsafe { msg_send![&*banner, setBackgroundColor: &*bg] };

    // Accessibility identifier — matches the SwiftUI banner.
    let a11y = NSString::from_str("disconnect.banner");
    let _: () = unsafe { msg_send![&*banner, setAccessibilityIdentifier: &*a11y] };

    let stack_frame = CGRect {
        origin: CGPoint {
            x: PADDING_H,
            y: PADDING_V,
        },
        size: CGSize {
            width: banner_frame.size.width - 2.0 * PADDING_H,
            height: banner_frame.size.height - 2.0 * PADDING_V,
        },
    };
    let stack: Retained<UIStackView> =
        unsafe { msg_send![mtm.alloc::<UIStackView>(), initWithFrame: stack_frame] };
    stack.setAxis(UILayoutConstraintAxis::Horizontal);
    stack.setAlignment(UIStackViewAlignment::Center);
    stack.setDistribution(UIStackViewDistribution::Fill);
    stack.setSpacing(SPACING);
    // Stack grows with the banner — flexible width + flexible height.
    let _: () = unsafe { msg_send![&*stack, setAutoresizingMask: (1u64 << 1) | (1u64 << 4)] };
    let _: () = unsafe { msg_send![&*banner, addSubview: &*stack] };

    // Reason label.
    let label: Retained<UILabel> = unsafe { msg_send![mtm.alloc::<UILabel>(), init] };
    let text_ns = NSString::from_str(message);
    let _: () = unsafe { msg_send![&*label, setText: &*text_ns] };
    let label_font: Retained<UIFont> =
        unsafe { msg_send![UIFont::class(), systemFontOfSize: LABEL_FONT_SIZE] };
    let _: () = unsafe { msg_send![&*label, setFont: &*label_font] };
    let _: () = unsafe { msg_send![&*label, setNumberOfLines: LABEL_MAX_LINES] };
    let label_fg = colors::shadcn_primary_foreground();
    let _: () = unsafe { msg_send![&*label, setTextColor: &*label_fg] };
    stack.addArrangedSubview(&label);

    // Reconnect button — borderedProminent equivalent.
    let cfg = UIButtonConfiguration::filledButtonConfiguration(mtm);
    let title_ns = NSString::from_str("Reconnect");
    let title_font: Retained<UIFont> = unsafe {
        msg_send![
            UIFont::class(),
            systemFontOfSize: BUTTON_FONT_SIZE,
            weight: FONT_WEIGHT_SEMIBOLD,
        ]
    };
    let font_attr_key = NSString::from_str("NSFont");
    let font_obj: Retained<AnyObject> = unsafe { Retained::cast_unchecked(title_font.clone()) };
    let attrs: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_retained_objects(&[&*font_attr_key], &[font_obj]);
    let attributed: Retained<NSAttributedString> = unsafe {
        NSAttributedString::initWithString_attributes(
            mtm.alloc::<NSAttributedString>(),
            &title_ns,
            Some(&attrs),
        )
    };
    cfg.setAttributedTitle(Some(&attributed));
    let insets = NSDirectionalEdgeInsets {
        top: 4.0,
        leading: 10.0,
        bottom: 4.0,
        trailing: 10.0,
    };
    cfg.setContentInsets(insets);
    cfg.setCornerStyle(objc2_ui_kit::UIButtonConfigurationCornerStyle::Fixed);
    let primary = colors::shadcn_primary();
    let primary_fg = colors::shadcn_primary_foreground();
    cfg.setBaseBackgroundColor(Some(&primary));
    cfg.setBaseForegroundColor(Some(&primary_fg));

    let bg_cfg = objc2_ui_kit::UIBackgroundConfiguration::clearConfiguration(mtm);
    bg_cfg.setCornerRadius(CORNER_RADIUS);
    bg_cfg.setBackgroundColor(Some(&primary));
    cfg.setBackground(&bg_cfg);

    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    let _: () = unsafe {
        msg_send![&*button,
            addTarget: target,
            action: action,
            forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE,
        ]
    };
    stack.addArrangedSubview(&button);

    banner
}
