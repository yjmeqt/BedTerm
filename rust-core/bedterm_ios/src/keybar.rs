//! Keybar accessory view for view2 — SwiftUI shadcn style.
//!
//! A horizontal row of "icon · label" chips matching the Swift
//! `BlockListComposer` footer: SF Symbol (size 11, weight medium) +
//! verbatim 12 pt label, muted-gray foreground, 8 h / 4 v padding,
//! no fill. Attached as `view2.inputAccessoryView` so it tracks the
//! keyboard above view2.
//!
//! Parity audit vs Swift `KeyBar.swift` + `KeyBarController.swift`
//! (see also `coordinator.rs` for the controller side):
//!
//! Chips present in Swift `KeyBar`:
//!   - Esc — present (`keybarEsc`); forwards `0x1B` through the session
//!     and clears the Ctrl latch.
//!   - Ctrl — present (`keybarCtrl`); latch toggles + tint, and when a
//!     session is attached drives Ctrl-XOR-0x20 on the next byte with a
//!     3 s auto-unlatch (`BtIosTerminalSession::set_ctrl_pending`).
//!   - Tab — present (`keybarTab`); inserts `\t` and clears the Ctrl
//!     latch (parity with Swift `KeyBarState`).
//!   - Newline — present via `ChipSet::WithNewline` (Swift renders it
//!     in the composer footer rather than the keybar itself; we put it
//!     here for the Rust shell + also via composer footer in
//!     `block_list_composer`). Inserts `\n`.
//!   - Dpad / keyboard toggle chips — Swift KeyBar exposes them via
//!     `onToggleDpad` / `onToggleKeyboard` closures. The Rust shell
//!     currently delegates dpad expansion to the in-VC composer
//!     (`block_list_composer`); a follow-up
//!     (`TODO(post-block-list-composer)` in `block_list_composer.rs`)
//!     lands the explicit toggle chips.
//!   - Shift / other sticky modifiers: Swift does not expose them —
//!     parity is "neither has Shift".
//!
//! Accessibility identifiers used by Swift (`keybar.esc`, `keybar.ctrl`,
//! `keybar.tab`, `keybar.newline`) are applied in `make_chip` below.
//! The toggle chips (`keybar.kbtoggle`, `keybar.dpadtoggle`) follow
//! once the dpad expand/collapse plumbing lands — see
//! `block_list_composer.rs::TODO(post-block-list-composer)`.

use crate::coordinator::BtIosKeyboardCoordinator;
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, sel, ClassType};
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

/// Which chips to include in the keybar. State1/State2 use `Standard`
/// (no newline chip; newline isn't meaningful when the input field is
/// single-line and the composer is hidden). State3 uses `WithNewline`
/// to expose the explicit newline chip on the action row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChipSet {
    Standard,
    WithNewline,
}

#[allow(dead_code)]
pub(crate) fn make_keybar(
    mtm: MainThreadMarker,
    coordinator: &BtIosKeyboardCoordinator,
) -> Retained<UIView> {
    make_keybar_with(mtm, coordinator, ChipSet::WithNewline)
}

pub(crate) fn make_keybar_with(
    mtm: MainThreadMarker,
    coordinator: &BtIosKeyboardCoordinator,
    set: ChipSet,
) -> Retained<UIView> {
    let bar_frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 320.0,
            height: BAR_HEIGHT,
        },
    };
    let wrapper: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: bar_frame] };
    let _: () = unsafe { msg_send![&*wrapper, setAutoresizingMask: AUTORESIZE_FLEXIBLE_W] };

    let bg: Retained<UIColor> =
        unsafe { msg_send![UIColor::class(), secondarySystemBackgroundColor] };
    let _: () = unsafe { msg_send![&*wrapper, setBackgroundColor: &*bg] };

    let stack: Retained<UIStackView> =
        unsafe { msg_send![mtm.alloc::<UIStackView>(), initWithFrame: bar_frame] };
    stack.setAxis(UILayoutConstraintAxis::Horizontal);
    stack.setAlignment(UIStackViewAlignment::Center);
    stack.setDistribution(UIStackViewDistribution::Fill);
    stack.setSpacing(0.0);
    let _: () = unsafe {
        msg_send![&*stack, setAutoresizingMask: AUTORESIZE_FLEXIBLE_W | AUTORESIZE_FLEXIBLE_H]
    };
    let _: () = unsafe { msg_send![&*wrapper, addSubview: &*stack] };

    let target: &AnyObject = coordinator.as_ref() as &AnyObject;

    let muted = muted_foreground();
    let tab = make_chip(mtm, ICON_TAB, "Tab", &muted, target, sel!(keybarTab));
    let esc = make_chip(mtm, ICON_ESC, "Esc", &muted, target, sel!(keybarEsc));
    let ctrl = make_chip(mtm, ICON_CTRL, "Ctrl", &muted, target, sel!(keybarCtrl));

    if set == ChipSet::WithNewline {
        let newline = make_chip(
            mtm,
            ICON_NEWLINE,
            "Newline",
            &muted,
            target,
            sel!(keybarNewline),
        );
        stack.addArrangedSubview(&newline);
    }
    stack.addArrangedSubview(&tab);
    stack.addArrangedSubview(&esc);
    stack.addArrangedSubview(&ctrl);

    // Trailing spacer view so chips left-align under .fill distribution.
    let spacer_frame = CGRect::default();
    let spacer: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: spacer_frame] };
    stack.addArrangedSubview(&spacer);

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
    let cfg = UIButtonConfiguration::plainButtonConfiguration(mtm);

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
    cfg.setBaseForegroundColor(Some(foreground));

    // `buttonWithType: 0` = `.custom`; setConfiguration overrides visuals.
    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    let _: () = unsafe {
        msg_send![&*button,
            addTarget: target,
            action: action,
            forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE,
        ]
    };
    // Accessibility identifier — match Swift KeyBar parity
    // (`keybar.tab`, `keybar.esc`, `keybar.ctrl`, `keybar.newline`).
    let identifier = match label {
        "Tab" => Some("keybar.tab"),
        "Esc" => Some("keybar.esc"),
        "Ctrl" => Some("keybar.ctrl"),
        "Newline" => Some("keybar.newline"),
        _ => None,
    };
    if let Some(ident) = identifier {
        let ns = NSString::from_str(ident);
        let _: () = unsafe { msg_send![&*button, setAccessibilityIdentifier: &*ns] };
    }
    button
}

/// Approximate `Color("ShadcnMutedForeground")` — system `secondaryLabel` is
/// adaptive (lighter on dark, darker on light) and visually matches the
/// shadcn token (0.451 light / 0.631 dark) closely enough.
fn muted_foreground() -> Retained<UIColor> {
    unsafe { msg_send![UIColor::class(), secondaryLabelColor] }
}
