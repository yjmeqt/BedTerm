//! Toast card factory — builds one toast `UIView` (icon · text · dismiss).
//!
//! Visual parity with `ToastCard` in `ToasterOverlay.swift`: `ShadcnCard`
//! fill, 1 pt `ShadcnBorder`, 10 pt radius, soft shadow. The leading icon
//! is a kind-tinted SF symbol (info / checkmark / exclamationmark), the
//! title is 16 pt semibold `ShadcnPrimary`, the optional description is
//! 13 pt `ShadcnMutedForeground`, optional action buttons trail under the
//! text, and a trailing xmark dismiss button shows for non-persistent
//! toasts.
//!
//! All widget construction follows the existing component idioms
//! (`action_chip.rs`, `keybar.rs`, `disconnect_banner.rs`).

use super::{ToastAction, ToastKind};
use crate::a11y;
use crate::design_system::colors;
use bedterm_app::geometry::{CGFloat, CGRect, CGSize};
use bedterm_app::l10n::t;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSAttributedString, NSDictionary, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIBackgroundConfiguration, UIButton, UIButtonConfiguration, UIColor,
    UIFont, UIImage, UIImageSymbolConfiguration, UIImageSymbolWeight, UILabel,
    UILayoutConstraintAxis, UIPanGestureRecognizer, UIStackView, UIStackViewAlignment,
    UIStackViewDistribution, UIView,
};

const CORNER_RADIUS: CGFloat = 10.0;
const BORDER_WIDTH: CGFloat = 1.0;
const SHADOW_OPACITY: f32 = 0.08;
const SHADOW_RADIUS: CGFloat = 12.0;
const TITLE_FONT_SIZE: CGFloat = 16.0;
const DESC_FONT_SIZE: CGFloat = 13.0;
const ACTION_FONT_SIZE: CGFloat = 13.0;
const ICON_SIZE: CGFloat = 13.0;
const DISMISS_ICON_SIZE: CGFloat = 11.0;
const CONTENT_SPACING: CGFloat = 10.0;
const ACTION_CORNER_RADIUS: CGFloat = 6.0;

const FONT_WEIGHT_MEDIUM: CGFloat = 0.23;
const FONT_WEIGHT_SEMIBOLD: CGFloat = 0.3;

/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;
/// `UILayoutConstraintAxisHorizontal` = 0.
const AXIS_HORIZONTAL: i64 = 0;
/// High content-hugging priority so the icon / dismiss button stay compact
/// and the text column absorbs the slack.
const HUGGING_REQUIRED: f32 = 1000.0;

/// Retained handles the toaster needs to keep: the card view and the
/// content stack it measures during layout.
pub(super) struct ToastCardViews {
    pub(super) card: Retained<UIView>,
    pub(super) content: Retained<UIStackView>,
}

/// Build a toast card. `id` tags the action / dismiss controls so the
/// toaster's shared selectors can resolve which toast fired.
#[allow(clippy::too_many_arguments)]
pub(super) fn make_toast_card(
    mtm: MainThreadMarker,
    id: u64,
    kind: ToastKind,
    title: &str,
    description: Option<&str>,
    persistent: bool,
    actions: &[ToastAction],
    target: &AnyObject,
    dismiss_sel: Sel,
    action_sel: Sel,
    pan_sel: Sel,
) -> ToastCardViews {
    let card: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: CGRect::default()] };
    card.setBackgroundColor(Some(&colors::shadcn_card()));

    // Rounded border + soft shadow on the layer. `masksToBounds` stays
    // false so the shadow renders; the background fill still respects the
    // corner radius, and the content stack is inset well inside the
    // corners so it doesn't need clipping.
    let layer: Retained<AnyObject> = unsafe { msg_send![&*card, layer] };
    let _: () = unsafe { msg_send![&*layer, setCornerRadius: CORNER_RADIUS] };
    let _: () = unsafe { msg_send![&*layer, setBorderWidth: BORDER_WIDTH] };
    let border_cg: *mut AnyObject = unsafe { msg_send![&*colors::shadcn_border(), CGColor] };
    let _: () = unsafe { msg_send![&*layer, setBorderColor: border_cg] };
    let black: Retained<UIColor> = unsafe { msg_send![UIColor::class(), blackColor] };
    let shadow_cg: *mut AnyObject = unsafe { msg_send![&*black, CGColor] };
    let _: () = unsafe { msg_send![&*layer, setShadowColor: shadow_cg] };
    let _: () = unsafe { msg_send![&*layer, setShadowOpacity: SHADOW_OPACITY] };
    let _: () = unsafe { msg_send![&*layer, setShadowRadius: SHADOW_RADIUS] };
    let _: () = unsafe { msg_send![&*layer, setShadowOffset: CGSize { width: 0.0, height: 4.0 }] };

    a11y::set_a11y_id(&*card as &AnyObject, "toast.card");
    let a11y_text = match description {
        Some(d) => format!("{title}. {d}"),
        None => title.to_string(),
    };
    let _: () =
        unsafe { msg_send![&*card, setAccessibilityLabel: &*NSString::from_str(&a11y_text)] };

    // Outer horizontal stack: icon · text column · dismiss.
    let content = UIStackView::new(mtm);
    content.setAxis(UILayoutConstraintAxis::Horizontal);
    content.setAlignment(UIStackViewAlignment::Top);
    content.setDistribution(UIStackViewDistribution::Fill);
    content.setSpacing(CONTENT_SPACING);
    let content_view: &UIView = unsafe { &*Retained::as_ptr(&content).cast() };
    let _: () = unsafe { msg_send![&*card, addSubview: content_view] };

    // Leading kind icon.
    let icon = make_icon_button(mtm, kind);
    set_hugging(&icon, HUGGING_REQUIRED);
    content.addArrangedSubview(&icon);

    // Text column: title, optional description, optional action row.
    let text_stack = UIStackView::new(mtm);
    text_stack.setAxis(UILayoutConstraintAxis::Vertical);
    text_stack.setAlignment(UIStackViewAlignment::Leading);
    text_stack.setDistribution(UIStackViewDistribution::Fill);
    text_stack.setSpacing(2.0);

    let title_label = make_label(mtm, title, TITLE_FONT_SIZE, FONT_WEIGHT_SEMIBOLD, true);
    text_stack.addArrangedSubview(&title_label);

    if let Some(desc) = description {
        let desc_label = make_label(mtm, desc, DESC_FONT_SIZE, 0.0, false);
        text_stack.addArrangedSubview(&desc_label);
    }

    if !actions.is_empty() {
        let action_row = UIStackView::new(mtm);
        action_row.setAxis(UILayoutConstraintAxis::Horizontal);
        action_row.setAlignment(UIStackViewAlignment::Center);
        action_row.setSpacing(8.0);
        for (index, action) in actions.iter().enumerate() {
            let tag = (id as i64) * 1000 + index as i64;
            let button = make_action_button(
                mtm,
                &action.title,
                action.destructive,
                tag,
                index,
                target,
                action_sel,
            );
            action_row.addArrangedSubview(&button);
        }
        text_stack.addArrangedSubview(&action_row);
    }
    content.addArrangedSubview(&text_stack);

    // Trailing dismiss button (non-persistent toasts only) + swipe-up.
    if !persistent {
        let dismiss = make_dismiss_button(mtm, id, target, dismiss_sel);
        set_hugging(&dismiss, HUGGING_REQUIRED);
        content.addArrangedSubview(&dismiss);

        let pan: Retained<UIPanGestureRecognizer> = unsafe {
            msg_send![mtm.alloc::<UIPanGestureRecognizer>(), initWithTarget: target, action: pan_sel]
        };
        let _: () = unsafe { msg_send![&*card, addGestureRecognizer: &*pan] };
    }

    ToastCardViews { card, content }
}

fn set_hugging(view: &UIView, priority: f32) {
    let _: () =
        unsafe { msg_send![view, setContentHuggingPriority: priority, forAxis: AXIS_HORIZONTAL] };
}

fn make_label(
    mtm: MainThreadMarker,
    text: &str,
    size: CGFloat,
    weight: CGFloat,
    primary: bool,
) -> Retained<UILabel> {
    let label = UILabel::new(mtm);
    label.setText(Some(&NSString::from_str(text)));
    let font: Retained<UIFont> = if weight == 0.0 {
        unsafe { msg_send![UIFont::class(), systemFontOfSize: size] }
    } else {
        unsafe { msg_send![UIFont::class(), systemFontOfSize: size, weight: weight] }
    };
    unsafe { label.setFont(Some(&font)) };
    let color = if primary {
        colors::shadcn_primary()
    } else {
        colors::shadcn_muted_foreground()
    };
    unsafe { label.setTextColor(Some(&color)) };
    label.setNumberOfLines(0);
    label
}

fn make_icon_button(mtm: MainThreadMarker, kind: ToastKind) -> Retained<UIButton> {
    let cfg = UIButtonConfiguration::plainButtonConfiguration(mtm);
    let symbol = icon_symbol(kind);
    let sym_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
        ICON_SIZE,
        UIImageSymbolWeight::Bold,
    );
    if let Some(image) = UIImage::systemImageNamed(&NSString::from_str(symbol)) {
        let configured: Retained<UIImage> =
            unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*sym_cfg] };
        cfg.setImage(Some(&configured));
    }
    cfg.setBaseForegroundColor(Some(&icon_color(kind)));
    cfg.setContentInsets(NSDirectionalEdgeInsets {
        top: 0.0,
        leading: 0.0,
        bottom: 0.0,
        trailing: 0.0,
    });
    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    let _: () = unsafe { msg_send![&*button, setUserInteractionEnabled: false] };
    button
}

fn make_dismiss_button(
    mtm: MainThreadMarker,
    id: u64,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIButton> {
    let cfg = UIButtonConfiguration::plainButtonConfiguration(mtm);
    let sym_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
        DISMISS_ICON_SIZE,
        UIImageSymbolWeight::Semibold,
    );
    if let Some(image) = UIImage::systemImageNamed(&NSString::from_str("xmark")) {
        let configured: Retained<UIImage> =
            unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*sym_cfg] };
        cfg.setImage(Some(&configured));
    }
    cfg.setBaseForegroundColor(Some(&colors::shadcn_muted_foreground()));
    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    let _: () = unsafe { msg_send![&*button, setTag: id as i64] };
    let _: () = unsafe {
        msg_send![&*button, addTarget: target, action: action, forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE]
    };
    a11y::set_a11y_id(&*button as &AnyObject, "toast.dismiss");
    let _: () =
        unsafe { msg_send![&*button, setAccessibilityLabel: &*NSString::from_str(&t("Dismiss"))] };
    button
}

fn make_action_button(
    mtm: MainThreadMarker,
    title: &str,
    destructive: bool,
    tag: i64,
    index: usize,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIButton> {
    let cfg = UIButtonConfiguration::plainButtonConfiguration(mtm);

    let title_ns = NSString::from_str(title);
    let font: Retained<UIFont> = unsafe {
        msg_send![UIFont::class(), systemFontOfSize: ACTION_FONT_SIZE, weight: FONT_WEIGHT_MEDIUM]
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
    cfg.setContentInsets(NSDirectionalEdgeInsets {
        top: 4.0,
        leading: 10.0,
        bottom: 4.0,
        trailing: 10.0,
    });

    // Destructive: clear fill + destructive text/stroke. Primary: filled.
    let bg_cfg = UIBackgroundConfiguration::clearConfiguration(mtm);
    bg_cfg.setCornerRadius(ACTION_CORNER_RADIUS);
    if destructive {
        let destructive_color = colors::shadcn_destructive();
        cfg.setBaseForegroundColor(Some(&destructive_color));
        bg_cfg.setStrokeColor(Some(&destructive_color));
        let _: () = unsafe { msg_send![&*bg_cfg, setStrokeWidth: BORDER_WIDTH] };
    } else {
        let primary = colors::shadcn_primary();
        let primary_fg = colors::shadcn_primary_foreground();
        cfg.setBaseForegroundColor(Some(&primary_fg));
        bg_cfg.setBackgroundColor(Some(&primary));
    }
    cfg.setBackground(&bg_cfg);

    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    let _: () = unsafe { msg_send![&*button, setTag: tag] };
    let _: () = unsafe {
        msg_send![&*button, addTarget: target, action: action, forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE]
    };
    a11y::set_a11y_id(&*button as &AnyObject, &format!("toast.action.{index}"));
    button
}

/// SF symbol name for each kind (parity with `ToastCard.iconSpec`).
fn icon_symbol(kind: ToastKind) -> &'static str {
    match kind {
        ToastKind::Info => "info",
        ToastKind::Success => "checkmark",
        ToastKind::Warning | ToastKind::Error => "exclamationmark",
    }
}

/// Tint colour for each kind's icon. Success/warning use the system
/// green/orange (no design token exists — matches the Swift `.green` /
/// `.orange`); info/error use the shadcn tokens.
fn icon_color(kind: ToastKind) -> Retained<UIColor> {
    match kind {
        ToastKind::Info => colors::shadcn_muted_foreground(),
        ToastKind::Success => unsafe { msg_send![UIColor::class(), systemGreenColor] },
        ToastKind::Warning => unsafe { msg_send![UIColor::class(), systemOrangeColor] },
        ToastKind::Error => colors::shadcn_destructive(),
    }
}
