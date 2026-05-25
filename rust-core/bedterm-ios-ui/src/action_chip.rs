//! Unified action chip used for the [send / composer / close-composer]
//! buttons. SF Symbol leading + 12 pt label trailing, compact insets.
//! Optionally `tint_filled = true` for a filled accent (systemBlue) chip
//! with white foreground.

use crate::geometry::CGFloat;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSAttributedString, NSDictionary, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIButton, UIButtonConfiguration, UIColor, UIFont, UIImage,
    UIImageSymbolConfiguration, UIImageSymbolWeight,
};

const FONT_SIZE: CGFloat = 12.0;
const SYMBOL_SIZE: CGFloat = 11.0;
const PADDING_H: CGFloat = 10.0;
const PADDING_V: CGFloat = 5.0;
const IMAGE_PADDING: CGFloat = 4.0;

/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;

/// Build the unified send/composer/close-composer chip.
///
/// `tint_filled`:
/// - `true`: `filledButtonConfiguration` with `systemBlue` background and
///   white foreground (the send / composer / close-composer "primary" look).
/// - `false`: `plainButtonConfiguration` with `secondaryLabel` foreground.
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

    let title_ns = NSString::from_str(label);
    let font: Retained<UIFont> = unsafe { msg_send![UIFont::class(), systemFontOfSize: FONT_SIZE] };
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

    let icon_ns = NSString::from_str(icon);
    let sym_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
        SYMBOL_SIZE,
        UIImageSymbolWeight::Medium,
    );
    if let Some(image) = UIImage::systemImageNamed(&icon_ns) {
        let configured: Retained<UIImage> =
            unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*sym_cfg] };
        cfg.setImage(Some(&configured));
    }
    cfg.setImagePadding(IMAGE_PADDING);

    let insets = NSDirectionalEdgeInsets {
        top: PADDING_V,
        leading: PADDING_H,
        bottom: PADDING_V,
        trailing: PADDING_H,
    };
    cfg.setContentInsets(insets);

    if tint_filled {
        let bg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), systemBlueColor] };
        let fg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), whiteColor] };
        cfg.setBaseBackgroundColor(Some(&bg));
        cfg.setBaseForegroundColor(Some(&fg));
    } else {
        let fg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), secondaryLabelColor] };
        cfg.setBaseForegroundColor(Some(&fg));
    }

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
