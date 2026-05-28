//! Block-list composer — port of `BlockListComposer.swift`.
//!
//! The unified bottom-bar composer shown in `.blockList` display
//! mode. Single UIView composed of three rows in a vertical stack:
//!
//!   1. Prompt-context chips (host / cwd / git branch / last exit).
//!   2. Composer text view (1–3 line auto-grow UITextView).
//!   3. Action row: keybar [Tab/Esc/Ctrl] + Run chip.
//!
//! Visuals: `ShadcnBackground` ground + a 1px `ShadcnBorder` top
//! divider, matching Swift.
//!
//! Not wired into `vc.rs` yet — that's W7. The factory is exported so
//! the integration wave can drop the view into the VC's bottom strip.
//!
//! TODO(post-block-list-composer): wire dpad expand/collapse — Swift's
//! BlockListComposer footer has a `chromeChip` toggle for the
//! directional pad which we omit here pending a shared dpad-state
//! observable in the Rust shell. Same follow-up lands the
//! keyboard-toggle chip + the remaining accessibility identifiers
//! (`composer.tab`, `composer.run`, `keybar.dpadtoggle`,
//! `keybar.kbtoggle`).

#![allow(dead_code)]

use crate::composer_text_view::{make_composer_text_view, set_placeholder};
use crate::coordinator::BtIosKeyboardCoordinator;
use crate::design_system::colors;
use crate::geometry::{CGFloat, CGRect};
use crate::keybar::{make_keybar_with, ChipSet};
use crate::prompt_context::PromptContext;
use crate::prompt_context_chips::{make_prompt_context_chips, refresh_prompt_context_chips};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel, ClassType};
use objc2_foundation::{MainThreadMarker, NSAttributedString, NSDictionary, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIBackgroundConfiguration, UIButton, UIButtonConfiguration, UIFont,
    UIImage, UIImageSymbolConfiguration, UIImageSymbolWeight, UILayoutConstraintAxis, UIStackView,
    UIStackViewAlignment, UIStackViewDistribution, UIView,
};

const HORIZONTAL_INSET: CGFloat = 16.0;
const VERTICAL_PADDING: CGFloat = 10.0;
const ROW_SPACING: CGFloat = 6.0;
const DIVIDER_HEIGHT: CGFloat = 1.0;
const RUN_FONT_SIZE: CGFloat = 13.0;
const RUN_SYMBOL_SIZE: CGFloat = 12.0;
const FONT_WEIGHT_SEMIBOLD: CGFloat = 0.3;
const FONT_WEIGHT_BOLD: CGFloat = 0.4;

/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;

/// Tag of the embedded prompt-context chip row — looked up by
/// `refresh_block_list_composer`.
const TAG_PROMPT_CHIPS: i64 = 0x7707_0003;

/// Build the block-list composer container view.
///
/// `coordinator` provides the keybar/Run targets. `initial_context`
/// seeds the prompt-context chip row.
pub(crate) fn make_block_list_composer(
    mtm: MainThreadMarker,
    coordinator: &BtIosKeyboardCoordinator,
    initial_context: &PromptContext,
) -> Retained<UIView> {
    let frame = CGRect::default();
    let wrapper: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: frame] };

    // Background: ShadcnBackground (dynamic).
    let bg = colors::shadcn_background();
    let _: () = unsafe { msg_send![&*wrapper, setBackgroundColor: &*bg] };

    // Top hairline border (ShadcnBorder dynamic).
    let border = colors::shadcn_border();
    let divider: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: frame] };
    let _: () = unsafe { msg_send![&*divider, setBackgroundColor: &*border] };
    let _: () =
        unsafe { msg_send![&*divider, setTranslatesAutoresizingMaskIntoConstraints: false] };
    let _: () = unsafe { msg_send![&*wrapper, addSubview: &*divider] };

    // Vertical stack of the three rows.
    let stack: Retained<UIStackView> =
        unsafe { msg_send![mtm.alloc::<UIStackView>(), initWithFrame: frame] };
    stack.setAxis(UILayoutConstraintAxis::Vertical);
    stack.setAlignment(UIStackViewAlignment::Fill);
    stack.setDistribution(UIStackViewDistribution::Fill);
    stack.setSpacing(ROW_SPACING);
    let _: () = unsafe { msg_send![&*stack, setTranslatesAutoresizingMaskIntoConstraints: false] };
    let _: () = unsafe { msg_send![&*wrapper, addSubview: &*stack] };

    // 1. Prompt-context chips row.
    let chips = make_prompt_context_chips(mtm, initial_context);
    let _: () = unsafe { msg_send![&*chips, setTag: TAG_PROMPT_CHIPS] };
    stack.addArrangedSubview(&chips);

    // 2. Composer text view.
    let composer = make_composer_text_view(mtm, coordinator.as_ref());
    set_placeholder(&composer.text_view, "Type a command — Enter to run");
    // Pin its height between 1- and 3-line clamps.
    let h_anchor: Retained<AnyObject> = unsafe { msg_send![&*composer.text_view, heightAnchor] };
    let min_h: Retained<AnyObject> = unsafe {
        msg_send![
            &*h_anchor,
            constraintGreaterThanOrEqualToConstant: composer.one_line_height,
        ]
    };
    let max_h: Retained<AnyObject> = unsafe {
        msg_send![
            &*h_anchor,
            constraintLessThanOrEqualToConstant: composer.three_line_height,
        ]
    };
    let _: () = unsafe { msg_send![&*min_h, setActive: true] };
    let _: () = unsafe { msg_send![&*max_h, setActive: true] };
    stack.addArrangedSubview(&composer.text_view);

    // 3. Action row — keybar (Standard chip set) + Run chip.
    let action_row = make_action_row(mtm, coordinator);
    stack.addArrangedSubview(&action_row);

    // Pin divider to top, stack to safe-area-ish padding.
    pin_divider(&divider, &wrapper);
    pin_stack(&stack, &wrapper);

    wrapper
}

/// Update the prompt-context chip row inside an existing composer.
pub(crate) fn refresh_block_list_composer(view: &UIView, context: &PromptContext) {
    let chips_obj: Option<Retained<AnyObject>> =
        unsafe { msg_send![view, viewWithTag: TAG_PROMPT_CHIPS] };
    let Some(chips_obj) = chips_obj else { return };
    let chips: Retained<UIView> = unsafe { Retained::cast_unchecked(chips_obj) };
    refresh_prompt_context_chips(&chips, context);
}

fn make_action_row(
    mtm: MainThreadMarker,
    coordinator: &BtIosKeyboardCoordinator,
) -> Retained<UIView> {
    let frame = CGRect::default();
    let row: Retained<UIStackView> =
        unsafe { msg_send![mtm.alloc::<UIStackView>(), initWithFrame: frame] };
    row.setAxis(UILayoutConstraintAxis::Horizontal);
    row.setAlignment(UIStackViewAlignment::Center);
    row.setDistribution(UIStackViewDistribution::Fill);
    row.setSpacing(8.0);

    // Keybar — Standard set (Tab/Esc/Ctrl). Newline is reached via the
    // composer's own multi-line text view (Return inserts \n).
    let keybar = make_keybar_with(mtm, coordinator, ChipSet::Standard);
    row.addArrangedSubview(&keybar);

    // Run chip — wired to coordinator.keybarSend:.
    let run = make_run_button(mtm, coordinator);
    row.addArrangedSubview(&run);

    let row_view: Retained<UIView> = unsafe { Retained::cast_unchecked(row) };
    row_view
}

fn make_run_button(
    mtm: MainThreadMarker,
    coordinator: &BtIosKeyboardCoordinator,
) -> Retained<UIButton> {
    let cfg = UIButtonConfiguration::filledButtonConfiguration(mtm);

    // 13pt semibold "Run".
    let title_ns = NSString::from_str("Run");
    let font: Retained<UIFont> = unsafe {
        msg_send![
            UIFont::class(),
            systemFontOfSize: RUN_FONT_SIZE,
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

    // 12pt bold return arrow icon.
    let icon_ns = NSString::from_str("return");
    let sym_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
        RUN_SYMBOL_SIZE,
        UIImageSymbolWeight::Bold,
    );
    if let Some(image) = UIImage::systemImageNamed(&icon_ns) {
        let configured: Retained<UIImage> =
            unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*sym_cfg] };
        cfg.setImage(Some(&configured));
    }
    cfg.setImagePadding(6.0);

    let insets = NSDirectionalEdgeInsets {
        top: 6.0,
        leading: 12.0,
        bottom: 6.0,
        trailing: 12.0,
    };
    cfg.setContentInsets(insets);

    // ShadcnPrimary fill + ShadcnPrimaryForeground text.
    let bg_cfg = UIBackgroundConfiguration::clearConfiguration(mtm);
    bg_cfg.setCornerRadius(6.0);
    let primary = colors::shadcn_primary();
    let primary_fg = colors::shadcn_primary_foreground();
    bg_cfg.setBackgroundColor(Some(&primary));
    cfg.setBackground(&bg_cfg);
    cfg.setBaseForegroundColor(Some(&primary_fg));

    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    let _: () = unsafe {
        msg_send![&*button,
            addTarget: coordinator.as_ref() as &AnyObject,
            action: sel!(keybarSend:),
            forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE,
        ]
    };
    // Hug content so the keybar (placed first) takes the slack.
    let _: () =
        unsafe { msg_send![&*button, setContentHuggingPriority: 760.0_f32, forAxis: 0_i64] };
    let _: () = unsafe {
        msg_send![&*button, setContentCompressionResistancePriority: 760.0_f32, forAxis: 0_i64]
    };
    // Suppress the unused-const lint for the bold weight reference.
    let _ = FONT_WEIGHT_BOLD;
    button
}

fn pin_divider(divider: &UIView, parent: &UIView) {
    let leading: Retained<AnyObject> = unsafe { msg_send![divider, leadingAnchor] };
    let trailing: Retained<AnyObject> = unsafe { msg_send![divider, trailingAnchor] };
    let top: Retained<AnyObject> = unsafe { msg_send![divider, topAnchor] };
    let height: Retained<AnyObject> = unsafe { msg_send![divider, heightAnchor] };

    let p_leading: Retained<AnyObject> = unsafe { msg_send![parent, leadingAnchor] };
    let p_trailing: Retained<AnyObject> = unsafe { msg_send![parent, trailingAnchor] };
    let p_top: Retained<AnyObject> = unsafe { msg_send![parent, topAnchor] };

    let c1: Retained<AnyObject> =
        unsafe { msg_send![&*leading, constraintEqualToAnchor: &*p_leading] };
    let c2: Retained<AnyObject> =
        unsafe { msg_send![&*trailing, constraintEqualToAnchor: &*p_trailing] };
    let c3: Retained<AnyObject> = unsafe { msg_send![&*top, constraintEqualToAnchor: &*p_top] };
    let c4: Retained<AnyObject> =
        unsafe { msg_send![&*height, constraintEqualToConstant: DIVIDER_HEIGHT] };
    for c in [&c1, &c2, &c3, &c4] {
        let _: () = unsafe { msg_send![&**c, setActive: true] };
    }
}

fn pin_stack(stack: &UIStackView, parent: &UIView) {
    let leading: Retained<AnyObject> = unsafe { msg_send![stack, leadingAnchor] };
    let trailing: Retained<AnyObject> = unsafe { msg_send![stack, trailingAnchor] };
    let top: Retained<AnyObject> = unsafe { msg_send![stack, topAnchor] };
    let bottom: Retained<AnyObject> = unsafe { msg_send![stack, bottomAnchor] };

    let p_leading: Retained<AnyObject> = unsafe { msg_send![parent, leadingAnchor] };
    let p_trailing: Retained<AnyObject> = unsafe { msg_send![parent, trailingAnchor] };
    let p_top: Retained<AnyObject> = unsafe { msg_send![parent, topAnchor] };
    let p_bottom: Retained<AnyObject> = unsafe { msg_send![parent, bottomAnchor] };

    let c1: Retained<AnyObject> = unsafe {
        msg_send![&*leading, constraintEqualToAnchor: &*p_leading, constant: HORIZONTAL_INSET]
    };
    let c2: Retained<AnyObject> = unsafe {
        msg_send![&*trailing, constraintEqualToAnchor: &*p_trailing, constant: -HORIZONTAL_INSET]
    };
    let c3: Retained<AnyObject> =
        unsafe { msg_send![&*top, constraintEqualToAnchor: &*p_top, constant: VERTICAL_PADDING] };
    let c4: Retained<AnyObject> = unsafe {
        msg_send![&*bottom, constraintEqualToAnchor: &*p_bottom, constant: -VERTICAL_PADDING]
    };
    for c in [&c1, &c2, &c3, &c4] {
        let _: () = unsafe { msg_send![&**c, setActive: true] };
    }
}
