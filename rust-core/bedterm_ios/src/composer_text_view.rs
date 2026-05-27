//! Composer text view — port of `ComposerTextView.swift`.
//!
//! Extracted out of `vc.rs` so the same factory powers both:
//!   - The shared `view2` composer the VC owns directly (single
//!     long-lived UITextView reused across State1/State3).
//!   - The composer text view embedded in `block_list_composer`
//!     (W7's block-list display mode chrome).
//!
//! Public surface (per task spec):
//! ```ignore
//! pub(crate) fn make_composer_text_view(mtm, delegate) -> ComposerTextView;
//! ```
//! returns the configured UITextView plus the parent-needed
//! one-line / three-line clamp heights (driven off the resolved
//! font's `lineHeight` + the text container insets).
//!
//! Mirrors the Swift behaviour:
//! - 17 pt system font (Swift uses `UIFont.systemFontSize`-monospaced
//!   for the legacy composer; the in-VC composer was already 17 pt
//!   plain — we keep the in-VC styling so the visual stays identical).
//! - `secondarySystemBackgroundColor` (with a `ShadcnInput` token
//!   override if the catalog gains it later).
//! - Placeholder via an overlay `UILabel` because UITextView lacks a
//!   built-in placeholder; visibility toggled by the delegate on
//!   `textViewDidChange:`.
//! - Multi-line by default: Return inserts `\n`. Swift's `onSubmit`
//!   path is a per-call-site override (block-list footer uses Run)
//!   and is not modelled here — vc.rs / block_list_composer wire
//!   their own Run chip that submits + clears.

#![allow(dead_code)]

use crate::design_system::colors;
use crate::geometry::{CGFloat, CGRect, UIEdgeInsets};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{UIColor, UIFont, UILabel, UITextView};

const FONT_SIZE: CGFloat = 17.0;
/// Tag used to find the placeholder label inside the text view.
const PLACEHOLDER_TAG: i64 = 0x7707_0001;

pub(crate) struct ComposerTextView {
    pub text_view: Retained<UITextView>,
    pub one_line_height: CGFloat,
    pub three_line_height: CGFloat,
}

/// Build a configured composer UITextView. The `delegate` parameter is
/// any `&AnyObject` that responds to the usual `UITextViewDelegate`
/// selectors (notably `textViewDidChange:`) — typically the owning VC.
pub(crate) fn make_composer_text_view(
    mtm: MainThreadMarker,
    delegate: &AnyObject,
) -> ComposerTextView {
    let zero = CGRect::default();
    let text_view: Retained<UITextView> =
        unsafe { msg_send![mtm.alloc::<UITextView>(), initWithFrame: zero] };

    // Background: ShadcnInput (dynamic; resolves light/dark at draw time).
    let bg = colors::shadcn_input();
    let _: () = unsafe { msg_send![&*text_view, setBackgroundColor: &*bg] };

    // Delegate.
    let _: () = unsafe { msg_send![&*text_view, setDelegate: delegate] };

    // Font (17 pt system).
    let font: Retained<UIFont> = unsafe { msg_send![UIFont::class(), systemFontOfSize: FONT_SIZE] };
    let _: () = unsafe { msg_send![&*text_view, setFont: &*font] };

    // Match Swift's "no autocorrect / no autocaps / no smart anything".
    // UITextInputTraits selectors take Int (the enum's raw value).
    // 1 = .no for autocorrection/spellCheck/smart*; 0 = .none for autocaps.
    let _: () = unsafe { msg_send![&*text_view, setAutocorrectionType: 1_i64] };
    let _: () = unsafe { msg_send![&*text_view, setAutocapitalizationType: 0_i64] };
    let _: () = unsafe { msg_send![&*text_view, setSmartQuotesType: 1_i64] };
    let _: () = unsafe { msg_send![&*text_view, setSmartDashesType: 1_i64] };
    let _: () = unsafe { msg_send![&*text_view, setSmartInsertDeleteType: 1_i64] };
    let _: () = unsafe { msg_send![&*text_view, setSpellCheckingType: 1_i64] };

    // Compute one-line / three-line heights from the resolved font.
    let line_h: CGFloat = unsafe { msg_send![&*font, lineHeight] };
    let inset: UIEdgeInsets = unsafe { msg_send![&*text_view, textContainerInset] };
    let one_line = line_h + inset.top + inset.bottom;
    let three_line = line_h * 3.0 + inset.top + inset.bottom;

    // Placeholder overlay. Swift pins it via Auto Layout to the
    // text view's leading/top — we match.
    let placeholder: Retained<UILabel> =
        unsafe { msg_send![mtm.alloc::<UILabel>(), initWithFrame: zero] };
    let _: () = unsafe { msg_send![&*placeholder, setFont: &*font] };
    let secondary_fg: Retained<UIColor> =
        unsafe { msg_send![UIColor::class(), secondaryLabelColor] };
    let _: () = unsafe { msg_send![&*placeholder, setTextColor: &*secondary_fg] };
    let _: () = unsafe { msg_send![&*placeholder, setUserInteractionEnabled: false] };
    let _: () = unsafe { msg_send![&*placeholder, setTag: PLACEHOLDER_TAG] };
    let _: () =
        unsafe { msg_send![&*placeholder, setTranslatesAutoresizingMaskIntoConstraints: false] };
    let _: () = unsafe { msg_send![&*text_view, addSubview: &*placeholder] };

    let leading: Retained<AnyObject> = unsafe { msg_send![&*placeholder, leadingAnchor] };
    let top: Retained<AnyObject> = unsafe { msg_send![&*placeholder, topAnchor] };
    let p_leading: Retained<AnyObject> = unsafe { msg_send![&*text_view, leadingAnchor] };
    let p_top: Retained<AnyObject> = unsafe { msg_send![&*text_view, topAnchor] };
    let c1: Retained<AnyObject> =
        unsafe { msg_send![&*leading, constraintEqualToAnchor: &*p_leading, constant: 5.0_f64] };
    let c2: Retained<AnyObject> =
        unsafe { msg_send![&*top, constraintEqualToAnchor: &*p_top, constant: 8.0_f64] };
    let _: () = unsafe { msg_send![&*c1, setActive: true] };
    let _: () = unsafe { msg_send![&*c2, setActive: true] };

    ComposerTextView {
        text_view,
        one_line_height: one_line,
        three_line_height: three_line,
    }
}

/// Set the placeholder text. The label is found via tag so the
/// caller doesn't need to thread it through.
pub(crate) fn set_placeholder(text_view: &UITextView, placeholder: &str) {
    let label_obj: Option<Retained<AnyObject>> =
        unsafe { msg_send![text_view, viewWithTag: PLACEHOLDER_TAG] };
    let Some(label_obj) = label_obj else { return };
    let label: Retained<UILabel> = unsafe { Retained::cast_unchecked(label_obj) };
    let ns = NSString::from_str(placeholder);
    let _: () = unsafe { msg_send![&*label, setText: &*ns] };
    refresh_placeholder(text_view);
}

/// Show / hide the placeholder based on whether the text view is
/// empty. Call from your `textViewDidChange:` handler.
pub(crate) fn refresh_placeholder(text_view: &UITextView) {
    let label_obj: Option<Retained<AnyObject>> =
        unsafe { msg_send![text_view, viewWithTag: PLACEHOLDER_TAG] };
    let Some(label_obj) = label_obj else { return };
    let label: Retained<UILabel> = unsafe { Retained::cast_unchecked(label_obj) };
    let text: Option<Retained<NSString>> = unsafe { msg_send![text_view, text] };
    let is_empty = text.map(|t| t.length() == 0).unwrap_or(true);
    let _: () = unsafe { msg_send![&*label, setHidden: !is_empty] };
}
