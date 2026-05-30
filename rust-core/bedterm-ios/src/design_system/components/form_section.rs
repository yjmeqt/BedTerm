//! Form section containers.
//!
//! * [`form_section`] — legacy inset-grouped section with an uppercased
//!   header above a hairline-bordered card. Kept for the Settings VC,
//!   which still uses the iOS Form aesthetic.
//!
//! * [`form_card`] — Shadcn card mirroring SwiftUI `ShadcnCard`: title
//!   (16 pt semibold) + optional description (13 pt muted) live inside
//!   the card, with the field rows beneath. 10 pt corner radius, 1 pt
//!   `ShadcnBorder` stroke, 16 pt internal padding, 14 pt row spacing.
//!   This is the container the connect-form VC composes.

use crate::design_system::components::header_label::{
    card_description_label, card_title_label, header_label,
};
use crate::design_system::{colors, spacing};
use bedterm_app::design_system::form_section_metrics::FormSectionMetrics;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::MainThreadMarker;
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UILayoutConstraintAxis, UIStackView, UIStackViewAlignment,
    UIStackViewDistribution, UIView,
};

/// Wraps `rows` in a vertical stack with a card background. If `header`
/// is supplied, an uppercased label is added above the card. Legacy
/// inset-grouped style; new screens should prefer [`form_card`].
pub fn form_section(
    mtm: MainThreadMarker,
    header: Option<&str>,
    rows: &[Retained<UIView>],
) -> Retained<UIView> {
    let outer = UIStackView::new(mtm);
    outer.setAxis(UILayoutConstraintAxis::Vertical);
    outer.setAlignment(UIStackViewAlignment::Fill);
    outer.setDistribution(UIStackViewDistribution::Fill);
    outer.setSpacing(spacing::SM);

    if let Some(text) = header {
        let label = header_label(mtm, text);
        outer.addArrangedSubview(&label);
    }

    let row_stack = UIStackView::new(mtm);
    row_stack.setAxis(UILayoutConstraintAxis::Vertical);
    row_stack.setAlignment(UIStackViewAlignment::Fill);
    row_stack.setDistribution(UIStackViewDistribution::Fill);
    row_stack.setSpacing(0.0);
    row_stack.setBackgroundColor(Some(&colors::shadcn_card()));
    for row in rows {
        row_stack.addArrangedSubview(row);
    }

    let layer: Retained<AnyObject> = unsafe { msg_send![&*row_stack, layer] };
    unsafe {
        let _: () = msg_send![&*layer, setCornerRadius: FormSectionMetrics::LEGACY_CORNER_RADIUS];
        let _: () = msg_send![&*layer, setBorderWidth: FormSectionMetrics::LEGACY_BORDER_WIDTH];
        let _: () = msg_send![&*layer, setMasksToBounds: true];
        let border = colors::shadcn_border();
        let cg: *const AnyObject = msg_send![&*border, CGColor];
        let _: () = msg_send![&*layer, setBorderColor: cg];
    }

    outer.addArrangedSubview(&row_stack);
    unsafe { Retained::cast_unchecked::<UIView>(outer) }
}

/// Build a Shadcn-style card containing a header (title + optional
/// description) and a vertical stack of `rows`. Mirrors the SwiftUI
/// `ShadcnCard` view exactly: 10 pt corner radius, 1 pt `ShadcnBorder`
/// stroke, `ShadcnCard` fill, 16 pt padding on all sides, 16 pt spacing
/// between the header block and the rows, 14 pt spacing between rows.
pub fn form_card(
    mtm: MainThreadMarker,
    title: &str,
    description: Option<&str>,
    rows: &[Retained<UIView>],
) -> Retained<UIView> {
    // Header block: title (always) + optional description.
    let header_stack = UIStackView::new(mtm);
    header_stack.setAxis(UILayoutConstraintAxis::Vertical);
    header_stack.setAlignment(UIStackViewAlignment::Leading);
    header_stack.setDistribution(UIStackViewDistribution::Fill);
    header_stack.setSpacing(FormSectionMetrics::HEADER_TITLE_DESCRIPTION_SPACING);
    header_stack.addArrangedSubview(&card_title_label(mtm, title));
    if let Some(desc) = description {
        header_stack.addArrangedSubview(&card_description_label(mtm, desc));
    }

    // Row body: vertical stack of caller-supplied rows.
    let row_stack = UIStackView::new(mtm);
    row_stack.setAxis(UILayoutConstraintAxis::Vertical);
    row_stack.setAlignment(UIStackViewAlignment::Fill);
    row_stack.setDistribution(UIStackViewDistribution::Fill);
    row_stack.setSpacing(FormSectionMetrics::ROW_SPACING);
    for row in rows {
        row_stack.addArrangedSubview(row);
    }

    // Outer card stack — applies internal padding via layout margins so
    // the 16 pt inset is visible on all sides.
    let card = UIStackView::new(mtm);
    card.setAxis(UILayoutConstraintAxis::Vertical);
    card.setAlignment(UIStackViewAlignment::Fill);
    card.setDistribution(UIStackViewDistribution::Fill);
    card.setSpacing(FormSectionMetrics::HEADER_TO_ROWS_SPACING);
    card.setLayoutMarginsRelativeArrangement(true);
    card.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
        top: FormSectionMetrics::CARD_PADDING,
        leading: FormSectionMetrics::CARD_PADDING,
        bottom: FormSectionMetrics::CARD_PADDING,
        trailing: FormSectionMetrics::CARD_PADDING,
    });
    card.setBackgroundColor(Some(&colors::shadcn_card()));
    card.addArrangedSubview(unsafe { &*(&*header_stack as *const UIStackView as *const UIView) });
    card.addArrangedSubview(unsafe { &*(&*row_stack as *const UIStackView as *const UIView) });

    // Apply rounded border via CALayer.
    let layer: Retained<AnyObject> = unsafe { msg_send![&*card, layer] };
    unsafe {
        let _: () = msg_send![&*layer, setCornerRadius: FormSectionMetrics::CARD_CORNER_RADIUS];
        let _: () = msg_send![&*layer, setBorderWidth: FormSectionMetrics::CARD_BORDER_WIDTH];
        let _: () = msg_send![&*layer, setMasksToBounds: true];
        let border = colors::shadcn_border();
        let cg: *const AnyObject = msg_send![&*border, CGColor];
        let _: () = msg_send![&*layer, setBorderColor: cg];
    }

    unsafe { Retained::cast_unchecked::<UIView>(card) }
}

/// Build a section block in the classic iOS grouped-form style: an
/// uppercase section header **above** a plain card (no in-card title), and
/// an optional muted description sentence **below** the card. The card
/// itself contains only the supplied `rows` and uses the same chrome as
/// [`form_card`] (10 pt radius, 1 pt `ShadcnBorder`, `ShadcnCard` fill,
/// 16 pt internal padding, 14 pt row spacing).
///
/// Header and description are indented horizontally by
/// `EXTERNAL_HEADER_INSET` so they align with the leading edge of in-card
/// text.
pub fn form_card_external_header(
    mtm: MainThreadMarker,
    header: &str,
    description: Option<&str>,
    rows: &[Retained<UIView>],
) -> Retained<UIView> {
    // -- Card body (rows only) -----------------------------------------
    let row_stack = UIStackView::new(mtm);
    row_stack.setAxis(UILayoutConstraintAxis::Vertical);
    row_stack.setAlignment(UIStackViewAlignment::Fill);
    row_stack.setDistribution(UIStackViewDistribution::Fill);
    row_stack.setSpacing(FormSectionMetrics::ROW_SPACING);
    for row in rows {
        row_stack.addArrangedSubview(row);
    }

    let card = UIStackView::new(mtm);
    card.setAxis(UILayoutConstraintAxis::Vertical);
    card.setAlignment(UIStackViewAlignment::Fill);
    card.setDistribution(UIStackViewDistribution::Fill);
    card.setSpacing(0.0);
    card.setLayoutMarginsRelativeArrangement(true);
    card.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
        top: FormSectionMetrics::CARD_PADDING,
        leading: FormSectionMetrics::CARD_PADDING,
        bottom: FormSectionMetrics::CARD_PADDING,
        trailing: FormSectionMetrics::CARD_PADDING,
    });
    card.setBackgroundColor(Some(&colors::shadcn_card()));
    card.addArrangedSubview(unsafe { &*(&*row_stack as *const UIStackView as *const UIView) });

    let layer: Retained<AnyObject> = unsafe { msg_send![&*card, layer] };
    unsafe {
        let _: () = msg_send![&*layer, setCornerRadius: FormSectionMetrics::CARD_CORNER_RADIUS];
        let _: () = msg_send![&*layer, setBorderWidth: FormSectionMetrics::CARD_BORDER_WIDTH];
        let _: () = msg_send![&*layer, setMasksToBounds: true];
        let border = colors::shadcn_border();
        let cg: *const AnyObject = msg_send![&*border, CGColor];
        let _: () = msg_send![&*layer, setBorderColor: cg];
    }

    // -- Outer stack: header / card / description ----------------------
    let outer = UIStackView::new(mtm);
    outer.setAxis(UILayoutConstraintAxis::Vertical);
    outer.setAlignment(UIStackViewAlignment::Fill);
    outer.setDistribution(UIStackViewDistribution::Fill);
    outer.setSpacing(0.0);

    // External header — uppercase, muted, indented.
    let header_label_view = header_label(mtm, header);
    let header_wrap = UIStackView::new(mtm);
    header_wrap.setAxis(UILayoutConstraintAxis::Vertical);
    header_wrap.setAlignment(UIStackViewAlignment::Fill);
    header_wrap.setDistribution(UIStackViewDistribution::Fill);
    header_wrap.setLayoutMarginsRelativeArrangement(true);
    header_wrap.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
        top: 0.0,
        leading: FormSectionMetrics::EXTERNAL_HEADER_INSET,
        bottom: FormSectionMetrics::EXTERNAL_HEADER_TO_CARD_SPACING,
        trailing: FormSectionMetrics::EXTERNAL_HEADER_INSET,
    });
    header_wrap.addArrangedSubview(&header_label_view);
    outer.addArrangedSubview(unsafe { &*(&*header_wrap as *const UIStackView as *const UIView) });

    outer.addArrangedSubview(unsafe { &*(&*card as *const UIStackView as *const UIView) });

    if let Some(text) = description {
        let desc_label = card_description_label(mtm, text);
        let desc_wrap = UIStackView::new(mtm);
        desc_wrap.setAxis(UILayoutConstraintAxis::Vertical);
        desc_wrap.setAlignment(UIStackViewAlignment::Fill);
        desc_wrap.setDistribution(UIStackViewDistribution::Fill);
        desc_wrap.setLayoutMarginsRelativeArrangement(true);
        desc_wrap.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
            top: FormSectionMetrics::CARD_TO_EXTERNAL_DESCRIPTION_SPACING,
            leading: FormSectionMetrics::EXTERNAL_HEADER_INSET,
            bottom: 0.0,
            trailing: FormSectionMetrics::EXTERNAL_HEADER_INSET,
        });
        desc_wrap.addArrangedSubview(&desc_label);
        outer.addArrangedSubview(unsafe { &*(&*desc_wrap as *const UIStackView as *const UIView) });
    }

    unsafe { Retained::cast_unchecked::<UIView>(outer) }
}
