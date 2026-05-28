//! Connecting overlay — UIKit port of `ConnectingOverlay.swift`.
//!
//! Full-bleed scrim shown while `TerminalSession.state == .connecting`. A
//! centred card holds a large `UIActivityIndicatorView`, a "Connecting to
//! <host>…" label, and a Cancel chip (matches `make_action_chip` styling,
//! with the `xmark` SF symbol). The caller (the VC) sizes the overlay to
//! its bounds; the inner stack auto-centres via autoresizing flags.

// Factory isn't wired into the VC yet (this wave only adds the factory).
// Suppress dead-code warnings — clippy is run with `-D warnings`.
#![allow(dead_code)]

use crate::action_chip::make_action_chip;
use crate::design_system::colors;
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    UIActivityIndicatorView, UIActivityIndicatorViewStyle, UIColor, UIFont, UILabel,
    UILayoutConstraintAxis, UIStackView, UIStackViewAlignment, UIStackViewDistribution, UIView,
};

const CARD_WIDTH: CGFloat = 260.0;
const CARD_HEIGHT: CGFloat = 200.0;
const STACK_SPACING: CGFloat = 16.0;
const LABEL_FONT_SIZE: CGFloat = 16.0; // .callout ~ 16pt
const LABEL_MAX_LINES: i64 = 2;
const SCRIM_ALPHA: f64 = 0.85;

/// All four autoresizing margins flexible — centres the card in its parent
/// bounds and keeps it centred when the parent resizes.
const AUTORESIZE_CENTER: u64 = (1u64 << 0) | (1u64 << 2) | (1u64 << 3) | (1u64 << 5);
/// Flexible width + flexible height — for the full-bleed scrim.
const AUTORESIZE_FILL: u64 = (1u64 << 1) | (1u64 << 4);

/// Build the full-bleed "Connecting to <host>…" overlay.
///
/// - `mtm`: main-thread marker (UIKit construction).
/// - `host_name`: localised host string interpolated into the message.
/// - `target` / `action`: target/selector wiring for the Cancel chip
///   (typically the coordinator + `connectingOverlayCancel:`).
pub(crate) fn make_connecting_overlay(
    mtm: MainThreadMarker,
    host_name: &str,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIView> {
    // Outer scrim — full-bleed, semi-opaque. Caller sets the frame; flexible
    // width + height keeps it filling the parent on rotation.
    let scrim_frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 320.0,
            height: 480.0,
        },
    };
    let scrim: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: scrim_frame] };
    let _: () = unsafe { msg_send![&*scrim, setAutoresizingMask: AUTORESIZE_FILL] };

    let bg_base = colors::shadcn_background();
    let bg: Retained<UIColor> =
        unsafe { msg_send![&*bg_base, colorWithAlphaComponent: SCRIM_ALPHA] };
    let _: () = unsafe { msg_send![&*scrim, setBackgroundColor: &*bg] };

    let a11y_overlay = NSString::from_str("terminal.connecting.overlay");
    let _: () = unsafe { msg_send![&*scrim, setAccessibilityIdentifier: &*a11y_overlay] };

    // Centred card hosting the spinner/label/cancel stack. Positioned with
    // autoresizing centre flags so it stays put on bounds changes.
    let card_x = (scrim_frame.size.width - CARD_WIDTH) / 2.0;
    let card_y = (scrim_frame.size.height - CARD_HEIGHT) / 2.0;
    let card_frame = CGRect {
        origin: CGPoint {
            x: card_x,
            y: card_y,
        },
        size: CGSize {
            width: CARD_WIDTH,
            height: CARD_HEIGHT,
        },
    };
    let stack: Retained<UIStackView> =
        unsafe { msg_send![mtm.alloc::<UIStackView>(), initWithFrame: card_frame] };
    stack.setAxis(UILayoutConstraintAxis::Vertical);
    stack.setAlignment(UIStackViewAlignment::Center);
    stack.setDistribution(UIStackViewDistribution::EqualSpacing);
    stack.setSpacing(STACK_SPACING);
    let _: () = unsafe { msg_send![&*stack, setAutoresizingMask: AUTORESIZE_CENTER] };
    let _: () = unsafe { msg_send![&*scrim, addSubview: &*stack] };

    // Spinner — large style, tinted to ShadcnPrimary, start animating.
    let spinner: Retained<UIActivityIndicatorView> = unsafe {
        msg_send![
            mtm.alloc::<UIActivityIndicatorView>(),
            initWithActivityIndicatorStyle: UIActivityIndicatorViewStyle::Large,
        ]
    };
    let tint = colors::shadcn_primary();
    let _: () = unsafe { msg_send![&*spinner, setColor: &*tint] };
    let _: () = unsafe { msg_send![&*spinner, startAnimating] };
    let _: () = unsafe { msg_send![&*spinner, setHidesWhenStopped: false] };
    stack.addArrangedSubview(&spinner);

    // "Connecting to <host>…" label.
    let label: Retained<UILabel> = unsafe { msg_send![mtm.alloc::<UILabel>(), init] };
    let text = format!("Connecting to {host_name}…");
    let text_ns = NSString::from_str(&text);
    let _: () = unsafe { msg_send![&*label, setText: &*text_ns] };
    let label_font: Retained<UIFont> =
        unsafe { msg_send![UIFont::class(), systemFontOfSize: LABEL_FONT_SIZE] };
    let _: () = unsafe { msg_send![&*label, setFont: &*label_font] };
    let _: () = unsafe { msg_send![&*label, setNumberOfLines: LABEL_MAX_LINES] };
    // .center alignment = 1.
    let _: () = unsafe { msg_send![&*label, setTextAlignment: 1_i64] };
    let label_fg = colors::shadcn_primary();
    let _: () = unsafe { msg_send![&*label, setTextColor: &*label_fg] };
    stack.addArrangedSubview(&label);

    // Cancel chip — reuse the existing action-chip style.
    let cancel = make_action_chip(mtm, "xmark", "Cancel", true, target, action);
    let a11y_cancel = NSString::from_str("terminal.connecting.cancel");
    let _: () = unsafe { msg_send![&*cancel, setAccessibilityIdentifier: &*a11y_cancel] };
    stack.addArrangedSubview(&cancel);

    scrim
}
