//! Unified action chip used for the [send / composer / close-composer]
//! buttons. SF Symbol leading + label trailing, pill background.
//!
//! Styling mirrors the Swift `runButton` (see `ComposerBar.swift` history):
//! 12 pt bold SF Symbol, 13 pt semibold label, 12h/6v padding, 6 pt
//! corner radius, `ShadcnPrimary` fill with `ShadcnPrimaryForeground` text.
//! Non-tinted variant uses `secondaryLabel` over a transparent ground.

use crate::design_system::colors;
use bedterm_app::geometry::CGFloat;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSAttributedString, NSDictionary, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIBackgroundConfiguration, UIButton, UIButtonConfiguration, UIColor,
    UIFont, UIImage, UIImageSymbolConfiguration, UIImageSymbolWeight,
};

const LABEL_FONT_SIZE: CGFloat = 13.0;
const SYMBOL_SIZE: CGFloat = 12.0;
const PADDING_H: CGFloat = 12.0;
const PADDING_V: CGFloat = 6.0;
const IMAGE_PADDING: CGFloat = 6.0;
const CORNER_RADIUS: CGFloat = 6.0;

// `UIFontWeight*` constants are typed `CGFloat`; UIKit defines them as raw
// floats matching the design-system weight axis.
const FONT_WEIGHT_SEMIBOLD: CGFloat = 0.3;
const FONT_WEIGHT_BOLD: CGFloat = 0.4;

/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;

/// Build the unified send/composer/close-composer chip.
///
/// `tint_filled`:
/// - `true`: `ShadcnPrimary` background + `ShadcnPrimaryForeground` text
///   (the send / composer / close-composer primary look).
/// - `false`: transparent background + `secondaryLabel` foreground.
pub(crate) fn make_action_chip(
    mtm: MainThreadMarker,
    icon: &str,
    label: &str,
    tint_filled: bool,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIButton> {
    let cfg = if tint_filled {
        UIButtonConfiguration::filledButtonConfiguration(mtm)
    } else {
        UIButtonConfiguration::plainButtonConfiguration(mtm)
    };

    // Title: 13 pt semibold. Skip entirely for icon-only chips
    // (Composer / Close pass `label = ""`).
    if !label.is_empty() {
        let title_ns = NSString::from_str(label);
        let font: Retained<UIFont> = unsafe {
            msg_send![
                UIFont::class(),
                systemFontOfSize: LABEL_FONT_SIZE,
                weight: FONT_WEIGHT_SEMIBOLD,
            ]
        };
        let font_attr_key = NSString::from_str("NSFont");
        let font_obj: Retained<AnyObject> = unsafe { Retained::cast_unchecked(font.clone()) };
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
    }

    // Icon: 12 pt bold SF Symbol.
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
    cfg.setImagePadding(IMAGE_PADDING);

    // 12h/6v content insets.
    let insets = NSDirectionalEdgeInsets {
        top: PADDING_V,
        leading: PADDING_H,
        bottom: PADDING_V,
        trailing: PADDING_H,
    };
    cfg.setContentInsets(insets);

    // Background: explicit UIBackgroundConfiguration so we control the
    // corner radius (filledButtonConfiguration defaults to a larger
    // capsule-ish radius). The Swift sibling uses `cornerRadius: 6`.
    let bg_cfg = UIBackgroundConfiguration::clearConfiguration(mtm);
    bg_cfg.setCornerRadius(CORNER_RADIUS);

    if tint_filled {
        let bg = colors::shadcn_primary();
        let fg = colors::shadcn_primary_foreground();
        bg_cfg.setBackgroundColor(Some(&bg));
        cfg.setBaseForegroundColor(Some(&fg));
    } else {
        let fg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), secondaryLabelColor] };
        cfg.setBaseForegroundColor(Some(&fg));
    }
    cfg.setBackground(&bg_cfg);

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

// Touch the bold weight constant so a future caller that wants a "bold"
// label can grab it from here instead of hard-coding the magic number.
#[allow(dead_code)]
pub(crate) const ACTION_CHIP_BOLD: CGFloat = FONT_WEIGHT_BOLD;
