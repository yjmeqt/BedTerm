//! Promoted from `action_chip`'s `tint_filled=true` branch — a UIButton
//! with `ShadcnPrimary` fill, `ShadcnPrimaryForeground` text, SF symbol
//! leading, 13 pt semibold label.
//!
//! For this wave `action_chip::make_action_chip` is the canonical
//! implementation and `primary_button(...)` delegates to it (per the
//! plan's "Pick (a)" decision). That keeps the existing call sites stable
//! and means upcoming Settings + Onboarding VCs can call the shared
//! design-system entry point.

use crate::action_chip::make_action_chip;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2_foundation::MainThreadMarker;
use objc2_ui_kit::UIButton;

/// SF symbol + label primary action button.
pub fn primary_button(
    mtm: MainThreadMarker,
    icon: &str,
    label: &str,
    target: &AnyObject,
    action: Sel,
) -> Retained<UIButton> {
    make_action_chip(mtm, icon, label, true, target, action)
}
