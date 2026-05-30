//! Reusable card-style choice button for onboarding picker steps.
//!
//! Port of the SwiftUI `OnboardingChoiceLabel` view (see
//! `BedTermKit/Sources/BedTermKit/Features/Onboarding/OnboardingChoiceLabel.swift`).
//! Bordered rounded rect with a leading title (`typography::headline`) above
//! a subtitle (`typography::callout` muted), and a trailing chevron. Uses
//! `ShadcnCard` background, `ShadcnBorder` 1pt border, 10pt corner radius.
//!
//! Lives under `onboarding/` because no other feature currently consumes a
//! two-line + chevron choice card; promote to `design_system/components/`
//! if a second caller appears.

use crate::design_system::{colors, spacing, typography};
use bedterm_app::geometry::CGFloat;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIButton, UIImage, UIImageSymbolConfiguration, UIImageSymbolWeight,
    UIImageView, UILabel, UILayoutConstraintAxis, UIStackView, UIStackViewAlignment,
    UIStackViewDistribution, UIView,
};

const CORNER_RADIUS: CGFloat = 10.0;
const BORDER_WIDTH: CGFloat = 1.0;
const CONTENT_PADDING: CGFloat = 16.0;
const CHEVRON_POINT_SIZE: CGFloat = 13.0;
/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;
/// `NSTextAlignment.left` = 0.
const TEXT_ALIGNMENT_LEFT: i64 = 0;

/// Build a card-style choice button. The button itself is the tap target;
/// title/subtitle/chevron are non-interactive subviews.
pub fn make_choice_button(
    mtm: MainThreadMarker,
    title: &str,
    subtitle: &str,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIButton> {
    // Button: type .custom so we control all subviews ourselves.
    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setBackgroundColor(Some(&colors::shadcn_card()));

    // Border + rounded corners via CALayer.
    let layer: Retained<AnyObject> = unsafe { msg_send![&*button, layer] };
    unsafe {
        let _: () = msg_send![&*layer, setCornerRadius: CORNER_RADIUS];
        let _: () = msg_send![&*layer, setBorderWidth: BORDER_WIDTH];
        let _: () = msg_send![&*layer, setMasksToBounds: true];
        let border = colors::shadcn_border();
        let cg: *const AnyObject = msg_send![&*border, CGColor];
        let _: () = msg_send![&*layer, setBorderColor: cg];
    }

    // Title label.
    let title_label = UILabel::new(mtm);
    title_label.setText(Some(&NSString::from_str(title)));
    unsafe {
        title_label.setFont(Some(&typography::headline()));
        title_label.setTextColor(Some(&colors::shadcn_primary()));
    }
    title_label.setNumberOfLines(0);
    let _: () = unsafe { msg_send![&*title_label, setTextAlignment: TEXT_ALIGNMENT_LEFT] };

    // Subtitle label.
    let subtitle_label = UILabel::new(mtm);
    subtitle_label.setText(Some(&NSString::from_str(subtitle)));
    unsafe {
        subtitle_label.setFont(Some(&typography::callout()));
        subtitle_label.setTextColor(Some(&colors::shadcn_muted_foreground()));
    }
    subtitle_label.setNumberOfLines(0);
    let _: () = unsafe { msg_send![&*subtitle_label, setTextAlignment: TEXT_ALIGNMENT_LEFT] };

    // Vertical text stack (title above subtitle).
    let text_stack = UIStackView::new(mtm);
    text_stack.setAxis(UILayoutConstraintAxis::Vertical);
    text_stack.setAlignment(UIStackViewAlignment::Leading);
    text_stack.setDistribution(UIStackViewDistribution::Fill);
    text_stack.setSpacing(spacing::XS);
    text_stack.addArrangedSubview(unsafe { &*(&*title_label as *const UILabel as *const UIView) });
    text_stack
        .addArrangedSubview(unsafe { &*(&*subtitle_label as *const UILabel as *const UIView) });

    // Chevron.
    let chevron_ns = NSString::from_str("chevron.right");
    let chevron_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
        CHEVRON_POINT_SIZE,
        UIImageSymbolWeight::Semibold,
    );
    let chevron_view = UIImageView::new(mtm);
    if let Some(image) = UIImage::systemImageNamed(&chevron_ns) {
        let configured: Retained<UIImage> =
            unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*chevron_cfg] };
        chevron_view.setImage(Some(&configured));
    }
    unsafe {
        let tint = colors::shadcn_muted_foreground();
        let _: () = msg_send![&*chevron_view, setTintColor: &*tint];
    }

    // Outer horizontal stack: [text_stack | chevron], padded.
    let outer = UIStackView::new(mtm);
    outer.setAxis(UILayoutConstraintAxis::Horizontal);
    outer.setAlignment(UIStackViewAlignment::Center);
    outer.setDistribution(UIStackViewDistribution::Fill);
    outer.setSpacing(spacing::MD);
    outer.setLayoutMarginsRelativeArrangement(true);
    outer.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
        top: CONTENT_PADDING,
        leading: CONTENT_PADDING,
        bottom: CONTENT_PADDING,
        trailing: CONTENT_PADDING,
    });
    outer.addArrangedSubview(unsafe { &*(&*text_stack as *const UIStackView as *const UIView) });
    outer.addArrangedSubview(unsafe { &*(&*chevron_view as *const UIImageView as *const UIView) });

    // Hugging priorities so chevron sticks to trailing edge.
    unsafe {
        let _: () = msg_send![&*text_stack, setContentHuggingPriority: 249_f32, forAxis: 0_i64];
        let _: () = msg_send![&*chevron_view, setContentHuggingPriority: 1000_f32, forAxis: 0_i64];
        let _: () = msg_send![
            &*chevron_view,
            setContentCompressionResistancePriority: 1000_f32,
            forAxis: 0_i64
        ];
    }

    // Mount the outer stack inside the button. Disable user interaction on
    // subviews so the button itself receives the tap.
    outer.setUserInteractionEnabled(false);
    let _: () = unsafe { msg_send![&*outer, setTranslatesAutoresizingMaskIntoConstraints: false] };
    let outer_view = unsafe { &*(&*outer as *const UIStackView as *const UIView) };
    button.addSubview(outer_view);

    // Pin outer to button edges (Auto Layout).
    unsafe {
        let top: Retained<AnyObject> = msg_send![&*outer, topAnchor];
        let bottom: Retained<AnyObject> = msg_send![&*outer, bottomAnchor];
        let leading: Retained<AnyObject> = msg_send![&*outer, leadingAnchor];
        let trailing: Retained<AnyObject> = msg_send![&*outer, trailingAnchor];
        let btn_top: Retained<AnyObject> = msg_send![&*button, topAnchor];
        let btn_bottom: Retained<AnyObject> = msg_send![&*button, bottomAnchor];
        let btn_leading: Retained<AnyObject> = msg_send![&*button, leadingAnchor];
        let btn_trailing: Retained<AnyObject> = msg_send![&*button, trailingAnchor];
        let c1: Retained<AnyObject> = msg_send![&*top, constraintEqualToAnchor: &*btn_top];
        let c2: Retained<AnyObject> = msg_send![&*bottom, constraintEqualToAnchor: &*btn_bottom];
        let c3: Retained<AnyObject> = msg_send![&*leading, constraintEqualToAnchor: &*btn_leading];
        let c4: Retained<AnyObject> =
            msg_send![&*trailing, constraintEqualToAnchor: &*btn_trailing];
        let _: () = msg_send![&*c1, setActive: true];
        let _: () = msg_send![&*c2, setActive: true];
        let _: () = msg_send![&*c3, setActive: true];
        let _: () = msg_send![&*c4, setActive: true];
    }

    // Wire the tap.
    let _: () = unsafe {
        msg_send![
            &*button,
            addTarget: target,
            action: action,
            forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE,
        ]
    };

    button
}
