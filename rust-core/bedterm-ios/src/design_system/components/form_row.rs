//! Horizontal "label leading, accessory trailing" row used inside form
//! sections. The caller supplies the accessory view (UISwitch, label, etc).

use crate::design_system::{colors, spacing, typography};
use objc2::rc::Retained;
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UILabel, UILayoutConstraintAxis, UIStackView, UIStackViewAlignment,
    UIStackViewDistribution, UIView,
};

/// Build a horizontal row with a leading label + trailing accessory.
/// Padding mirrors the iOS grouped-form metrics (LG horizontal, MD
/// vertical). The accessory view is added directly — give it an
/// intrinsic content size or pin it before mounting.
pub fn form_row(mtm: MainThreadMarker, label: &str, accessory: &UIView) -> Retained<UIView> {
    let label_view = UILabel::new(mtm);
    label_view.setText(Some(&NSString::from_str(label)));
    unsafe {
        label_view.setFont(Some(&typography::body()));
        label_view.setTextColor(Some(&colors::shadcn_primary()));
    }

    let stack = UIStackView::new(mtm);
    stack.setAxis(UILayoutConstraintAxis::Horizontal);
    stack.setAlignment(UIStackViewAlignment::Center);
    stack.setDistribution(UIStackViewDistribution::Fill);
    stack.setSpacing(spacing::MD);
    stack.setLayoutMarginsRelativeArrangement(true);
    stack.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
        top: spacing::MD,
        leading: spacing::LG,
        bottom: spacing::MD,
        trailing: spacing::LG,
    });
    stack.addArrangedSubview(&label_view);
    stack.addArrangedSubview(accessory);

    // Hairline separator hugs the bottom (caller can hide if it's the
    // last row in a section).
    let separator = UIView::new(mtm);
    separator.setBackgroundColor(Some(&colors::shadcn_border()));
    stack.addSubview(&separator);

    unsafe { Retained::cast_unchecked::<UIView>(stack) }
}
