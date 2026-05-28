//! Multi-line muted footnote shown beneath a form section's rows.
//!
//! Mirrors the SwiftUI `Form` footer-text idiom — small body copy that
//! explains what the toggles above it do. Wrapped in a margined container
//! view so the caller can drop it straight into a `form_section`'s rows
//! slice without needing to add layout margins itself.

use crate::design_system::{colors, spacing, typography};
use objc2::rc::Retained;
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UILabel, UILayoutConstraintAxis, UIStackView, UIStackViewAlignment,
    UIStackViewDistribution, UIView,
};

/// Build a wrapped 12 pt muted-foreground label suitable for use as a
/// section footer. Uses 0 `numberOfLines` so the text wraps naturally to
/// the section's width.
pub fn footer_label(mtm: MainThreadMarker, text: &str) -> Retained<UIView> {
    let label = UILabel::new(mtm);
    label.setText(Some(&NSString::from_str(text)));
    label.setNumberOfLines(0);
    unsafe {
        label.setFont(Some(&typography::caption()));
        label.setTextColor(Some(&colors::shadcn_muted_foreground()));
    }

    // Stack wrapper so we can apply directional margins (matches the
    // `form_row` rhythm) — keeps the footer visually aligned with the
    // label column above it.
    let stack = UIStackView::new(mtm);
    stack.setAxis(UILayoutConstraintAxis::Vertical);
    stack.setAlignment(UIStackViewAlignment::Fill);
    stack.setDistribution(UIStackViewDistribution::Fill);
    stack.setLayoutMarginsRelativeArrangement(true);
    stack.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
        top: spacing::SM,
        leading: spacing::LG,
        bottom: spacing::SM,
        trailing: spacing::LG,
    });
    stack.addArrangedSubview(&label);

    unsafe { Retained::cast_unchecked::<UIView>(stack) }
}
