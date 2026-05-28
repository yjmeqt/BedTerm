//! UIFont factories. Centralises the size + weight choices the Rust UI
//! makes so future Settings + Onboarding VCs compose the same ladder.

#![cfg(target_os = "ios")]
// The ladder is reserved for the upcoming Settings (W23b) + Onboarding
// (W23c) Rust VCs; only `label_small` and `button_label` have call sites
// today (via the components below). Allow dead code here until those
// VCs wire up the rest.
#![allow(dead_code)]

use crate::geometry::CGFloat;
use objc2::rc::Retained;
use objc2::{msg_send, ClassType};
use objc2_ui_kit::UIFont;

// UIFontWeight* are raw-float constants (the framework defines them as
// `extern const CGFloat`). The values below match `UIFontWeightRegular`,
// `UIFontWeightMedium`, `UIFontWeightSemibold`, `UIFontWeightBold`.
pub const WEIGHT_REGULAR: CGFloat = 0.0;
pub const WEIGHT_MEDIUM: CGFloat = 0.23;
pub const WEIGHT_SEMIBOLD: CGFloat = 0.3;
pub const WEIGHT_BOLD: CGFloat = 0.4;

/// `+[UIFont systemFontOfSize:weight:]`.
pub fn system(size: CGFloat, weight: CGFloat) -> Retained<UIFont> {
    unsafe {
        msg_send![
            UIFont::class(),
            systemFontOfSize: size,
            weight: weight,
        ]
    }
}

/// `+[UIFont monospacedSystemFontOfSize:weight:]`.
pub fn monospaced(size: CGFloat, weight: CGFloat) -> Retained<UIFont> {
    unsafe {
        msg_send![
            UIFont::class(),
            monospacedSystemFontOfSize: size,
            weight: weight,
        ]
    }
}

/// 17 pt regular — body copy.
pub fn body() -> Retained<UIFont> {
    system(17.0, WEIGHT_REGULAR)
}

/// 13 pt regular — callout / chip labels.
pub fn callout() -> Retained<UIFont> {
    system(13.0, WEIGHT_REGULAR)
}

/// 12 pt regular — caption / footnote.
pub fn caption() -> Retained<UIFont> {
    system(12.0, WEIGHT_REGULAR)
}

/// 16 pt medium — matches SwiftUI `.callout.weight(.medium)`. Used for
/// list-row primary titles (HostRow parity).
pub fn callout_medium() -> Retained<UIFont> {
    system(16.0, WEIGHT_MEDIUM)
}

/// 13 pt regular — matches SwiftUI `.footnote`. Used for list-row
/// subtitles + secondary copy.
pub fn footnote() -> Retained<UIFont> {
    system(13.0, WEIGHT_REGULAR)
}

/// 17 pt semibold — section header / navigation accents.
pub fn headline() -> Retained<UIFont> {
    system(17.0, WEIGHT_SEMIBOLD)
}

/// 12 pt medium — small label (Settings form headers etc).
pub fn label_small() -> Retained<UIFont> {
    system(12.0, WEIGHT_MEDIUM)
}

/// 13 pt semibold — primary button label (action chip filled variant).
pub fn button_label() -> Retained<UIFont> {
    system(13.0, WEIGHT_SEMIBOLD)
}
