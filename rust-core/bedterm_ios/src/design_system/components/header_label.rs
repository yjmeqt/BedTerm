//! Label helpers used inside form sections.
//!
//! `header_label` — uppercased 12 pt medium muted label (legacy inset-
//! grouped header style; retained for callers that haven't migrated).
//!
//! `field_label` — uppercased 13 pt medium muted-foreground label used
//! inside cards above each text-field. Matches the classic iOS grouped-
//! form aesthetic (small-caps grey) confirmed by the SwiftUI baseline +
//! user screenshots, not the previous title-case Shadcn variant.
//!
//! `card_title_label` — 16 pt semibold ShadcnPrimary label; mirrors the
//! `ShadcnCard` header (`.callout.weight(.semibold)`).
//!
//! `card_description_label` — multi-line 13 pt muted-foreground; mirrors
//! the optional description string under the `ShadcnCard` title.

use crate::design_system::{colors, typography};
use objc2::rc::Retained;
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::UILabel;

/// Build an uppercased 12 pt medium label tinted to the muted-foreground
/// token. Caller is responsible for layout (typically pinned to the top of
/// a form section with `LG` leading + trailing margin).
pub fn header_label(mtm: MainThreadMarker, text: &str) -> Retained<UILabel> {
    let label = UILabel::new(mtm);
    let ns = NSString::from_str(&text.to_uppercase());
    label.setText(Some(&ns));
    unsafe {
        label.setFont(Some(&typography::label_small()));
        label.setTextColor(Some(&colors::shadcn_muted_foreground()));
    }
    label
}

/// Build an uppercased 13 pt medium muted-foreground label for use
/// inside card rows above each text-field. The label is shouted into
/// upper-case to match the classic iOS grouped-form / small-caps
/// aesthetic (FIELD_NAME) that the SwiftUI baseline + user screenshots
/// confirm — not the previous title-case primary-tinted variant.
pub fn field_label(mtm: MainThreadMarker, text: &str) -> Retained<UILabel> {
    let label = UILabel::new(mtm);
    let ns = NSString::from_str(&text.to_uppercase());
    label.setText(Some(&ns));
    unsafe {
        label.setFont(Some(&typography::system(13.0, typography::WEIGHT_MEDIUM)));
        label.setTextColor(Some(&colors::shadcn_muted_foreground()));
    }
    label
}

/// Build a 16 pt semibold `ShadcnPrimary` label — mirrors `ShadcnCard`'s
/// title (`.callout.weight(.semibold)`).
pub fn card_title_label(mtm: MainThreadMarker, text: &str) -> Retained<UILabel> {
    let label = UILabel::new(mtm);
    label.setText(Some(&NSString::from_str(text)));
    unsafe {
        label.setFont(Some(&typography::system(16.0, typography::WEIGHT_SEMIBOLD)));
        label.setTextColor(Some(&colors::shadcn_primary()));
    }
    label
}

/// Build a multi-line 13 pt regular `ShadcnMutedForeground` label —
/// mirrors `ShadcnCard`'s optional description (`.footnote` muted).
pub fn card_description_label(mtm: MainThreadMarker, text: &str) -> Retained<UILabel> {
    let label = UILabel::new(mtm);
    label.setText(Some(&NSString::from_str(text)));
    label.setNumberOfLines(0);
    unsafe {
        label.setFont(Some(&typography::footnote()));
        label.setTextColor(Some(&colors::shadcn_muted_foreground()));
    }
    label
}
