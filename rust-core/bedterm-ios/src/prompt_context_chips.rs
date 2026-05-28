//! Prompt-context chip row — port of `BlockListComposer+Chips.swift`
//! style + the `promptStrip` builder from `BlockListComposer.swift`.
//!
//! A small horizontal strip rendered above the composer input that
//! surfaces the shell's prompt context: host, cwd, current git branch,
//! and the last command's exit code. The Swift v1 hides everything
//! except `cwd` (the host / git / exit chips are wired through data
//! but not yet shown); the Rust port keeps that invariant — we build
//! all four chip labels but only the ones with a non-empty value are
//! mounted in the stack.
//!
//! Style matches Swift `BlockListComposer.chip(icon:text:)`:
//! - icon: SF Symbol 10pt medium, leading
//! - cwd / branch label: 11pt monospaced (Swift's `.system(size:11, design:.monospaced)`)
//! - host / exit label: 11pt regular
//! - foreground: `ShadcnMutedForeground`
//! - background: `ShadcnMutedForeground` @ 8% opacity, 5pt corner radius
//! - padding: 6h / 3v
//!
//! Cwd display rule (mirrors `displayCwd`): truncate to 28 chars from
//! the head with a leading ellipsis when longer.
//!
//! Note: spec asks for 12pt; Swift currently uses 11pt — we follow
//! Swift since the contract is "mirror Swift's exact visual".

#![allow(dead_code)]

use crate::design_system::colors;
use crate::geometry::{CGFloat, CGRect};
use crate::prompt_context::PromptContext;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSAttributedString, NSDictionary, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIBackgroundConfiguration, UIButton, UIButtonConfiguration, UIColor,
    UIFont, UIImage, UIImageSymbolConfiguration, UIImageSymbolWeight, UILayoutConstraintAxis,
    UIStackView, UIStackViewAlignment, UIStackViewDistribution, UIView,
};

const ICON_FONT_SIZE: CGFloat = 10.0;
const LABEL_FONT_SIZE: CGFloat = 11.0;
const PADDING_H: CGFloat = 6.0;
const PADDING_V: CGFloat = 3.0;
const CORNER_RADIUS: CGFloat = 5.0;
const ROW_SPACING: CGFloat = 6.0;
const ROW_HEIGHT: CGFloat = 22.0;
const CWD_MAX_CHARS: usize = 28;

/// Tag used to find the inner stack view when refreshing.
const STACK_TAG: i64 = 0x7707_0002;

/// Build a horizontal chip row showing prompt context. The returned
/// view is unparented; callers add it to their own stack / view.
pub(crate) fn make_prompt_context_chips(
    mtm: MainThreadMarker,
    context: &PromptContext,
) -> Retained<UIView> {
    let frame = CGRect::default();
    let wrapper: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: frame] };

    let stack: Retained<UIStackView> =
        unsafe { msg_send![mtm.alloc::<UIStackView>(), initWithFrame: frame] };
    stack.setAxis(UILayoutConstraintAxis::Horizontal);
    stack.setAlignment(UIStackViewAlignment::Center);
    stack.setDistribution(UIStackViewDistribution::Fill);
    stack.setSpacing(ROW_SPACING);
    let _: () = unsafe { msg_send![&*stack, setTag: STACK_TAG] };
    let _: () = unsafe { msg_send![&*stack, setTranslatesAutoresizingMaskIntoConstraints: false] };
    let _: () = unsafe { msg_send![&*wrapper, addSubview: &*stack] };

    pin_edges(&stack, &wrapper);
    populate_stack(mtm, &stack, context);

    wrapper
}

/// Rebuild the chip row in place from a new `PromptContext`. Cheap —
/// the row has at most 4 chips. Looks up the inner stack via its tag.
pub(crate) fn refresh_prompt_context_chips(view: &UIView, context: &PromptContext) {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let stack_obj: Option<Retained<AnyObject>> = unsafe { msg_send![view, viewWithTag: STACK_TAG] };
    let Some(stack_obj) = stack_obj else { return };
    let stack: Retained<UIStackView> = unsafe { Retained::cast_unchecked(stack_obj) };
    // Tear down existing arranged subviews.
    let subviews: Retained<objc2_foundation::NSArray<UIView>> =
        unsafe { msg_send![&*stack, arrangedSubviews] };
    let count: usize = unsafe { msg_send![&*subviews, count] };
    for i in 0..count {
        let sv: Retained<UIView> = unsafe { msg_send![&*subviews, objectAtIndex: i] };
        let _: () = unsafe { msg_send![&*stack, removeArrangedSubview: &*sv] };
        let _: () = unsafe { msg_send![&*sv, removeFromSuperview] };
    }
    populate_stack(mtm, &stack, context);
}

fn populate_stack(mtm: MainThreadMarker, stack: &UIStackView, context: &PromptContext) {
    if let Some(host) = context.host.as_deref().filter(|s| !s.is_empty()) {
        let chip = make_chip(mtm, "server.rack", host, false);
        stack.addArrangedSubview(&chip);
    }
    if let Some(cwd) = display_cwd(context.cwd.as_deref()) {
        let chip = make_chip(mtm, "folder", &cwd, true);
        stack.addArrangedSubview(&chip);
    }
    if let Some(branch) = context.git_branch.as_deref().filter(|s| !s.is_empty()) {
        let chip = make_chip(mtm, "arrow.triangle.branch", branch, true);
        stack.addArrangedSubview(&chip);
    }
    if let Some(code) = context.last_exit_code {
        let chip = make_chip(mtm, "checkmark.seal", &format!("{}", code), false);
        stack.addArrangedSubview(&chip);
    }
    // Trailing spacer so chips left-align under .fill distribution.
    let spacer_frame = CGRect::default();
    let spacer: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: spacer_frame] };
    stack.addArrangedSubview(&spacer);
}

fn display_cwd(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    if raw.is_empty() {
        return None;
    }
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() <= CWD_MAX_CHARS {
        return Some(raw.to_owned());
    }
    let suffix: String = chars[chars.len() - (CWD_MAX_CHARS - 1)..].iter().collect();
    Some(format!("…{}", suffix))
}

fn make_chip(
    mtm: MainThreadMarker,
    icon: &str,
    label: &str,
    monospaced: bool,
) -> Retained<UIButton> {
    let cfg = UIButtonConfiguration::plainButtonConfiguration(mtm);

    let title_ns = NSString::from_str(label);
    let font: Retained<UIFont> = if monospaced {
        unsafe {
            msg_send![
                UIFont::class(),
                monospacedSystemFontOfSize: LABEL_FONT_SIZE,
                weight: 0.0_f64,
            ]
        }
    } else {
        unsafe { msg_send![UIFont::class(), systemFontOfSize: LABEL_FONT_SIZE] }
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

    let icon_ns = NSString::from_str(icon);
    let sym_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
        ICON_FONT_SIZE,
        UIImageSymbolWeight::Medium,
    );
    if let Some(image) = UIImage::systemImageNamed(&icon_ns) {
        let configured: Retained<UIImage> =
            unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*sym_cfg] };
        cfg.setImage(Some(&configured));
    }
    cfg.setImagePadding(4.0);

    let insets = NSDirectionalEdgeInsets {
        top: PADDING_V,
        leading: PADDING_H,
        bottom: PADDING_V,
        trailing: PADDING_H,
    };
    cfg.setContentInsets(insets);

    let fg = colors::shadcn_muted_foreground();
    cfg.setBaseForegroundColor(Some(&fg));

    // 8% muted-foreground background pill.
    let bg_cfg = UIBackgroundConfiguration::clearConfiguration(mtm);
    bg_cfg.setCornerRadius(CORNER_RADIUS);
    let bg_tinted: Retained<UIColor> =
        unsafe { msg_send![&*fg, colorWithAlphaComponent: 0.08_f64] };
    bg_cfg.setBackgroundColor(Some(&bg_tinted));
    cfg.setBackground(&bg_cfg);

    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    // Display-only — no target/action.
    let _: () = unsafe { msg_send![&*button, setUserInteractionEnabled: false] };
    button
}

fn pin_edges(child: &UIStackView, parent: &UIView) {
    // Use Auto Layout — parent has no fixed size yet (it's added to a
    // vertical stack), and we want the chip row to fill its width.
    let leading: Retained<AnyObject> = unsafe { msg_send![child, leadingAnchor] };
    let trailing: Retained<AnyObject> = unsafe { msg_send![child, trailingAnchor] };
    let top: Retained<AnyObject> = unsafe { msg_send![child, topAnchor] };
    let bottom: Retained<AnyObject> = unsafe { msg_send![child, bottomAnchor] };
    let p_leading: Retained<AnyObject> = unsafe { msg_send![parent, leadingAnchor] };
    let p_trailing: Retained<AnyObject> = unsafe { msg_send![parent, trailingAnchor] };
    let p_top: Retained<AnyObject> = unsafe { msg_send![parent, topAnchor] };
    let p_bottom: Retained<AnyObject> = unsafe { msg_send![parent, bottomAnchor] };

    let c1: Retained<AnyObject> =
        unsafe { msg_send![&*leading, constraintEqualToAnchor: &*p_leading] };
    let c2: Retained<AnyObject> =
        unsafe { msg_send![&*trailing, constraintEqualToAnchor: &*p_trailing] };
    let c3: Retained<AnyObject> = unsafe { msg_send![&*top, constraintEqualToAnchor: &*p_top] };
    let c4: Retained<AnyObject> =
        unsafe { msg_send![&*bottom, constraintEqualToAnchor: &*p_bottom] };
    for c in [&c1, &c2, &c3, &c4] {
        let _: () = unsafe { msg_send![&**c, setActive: true] };
    }
}

/// Intrinsic height of the chip row — used by parents that lay out
/// manually (e.g. `block_list_composer`).
pub(crate) const PROMPT_CONTEXT_ROW_HEIGHT: CGFloat = ROW_HEIGHT;
