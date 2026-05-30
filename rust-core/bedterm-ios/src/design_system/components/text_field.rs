//! Labelled `UITextField` row used in the W24c connect-form Rust VC.
//!
//! Renders a column of `[label] / [bordered field]` so the caller can drop
//! the returned `UIView` directly into a vertical form stack. The
//! [`TextFieldHandle`] gives the caller a +1 retained `UITextField` so
//! they can install delegates, read/write text, and make it first responder.
//!
//! Visual rhythm tracks the SwiftUI `ShadcnTextField`: 8 pt corner radius,
//! 1 pt `ShadcnBorder`/`ShadcnInput` border, 12 pt horizontal inner
//! padding, 36 pt intrinsic height. See
//! [`bedterm_app::design_system::text_field_metrics::TextFieldMetrics`] for the
//! authoritative constants.

use crate::design_system::components::header_label::field_label;
use crate::design_system::{colors, typography};
use bedterm_app::design_system::text_field_metrics::TextFieldMetrics;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    UILayoutConstraintAxis, UIStackView, UIStackViewAlignment, UIStackViewDistribution,
    UITextField, UIView,
};

/// Handle returned alongside the row view so callers can read / write the
/// current text and (optionally) install a delegate.
pub struct TextFieldHandle {
    pub field: Retained<UITextField>,
}

impl TextFieldHandle {
    /// Replace the field's text. Safe to call before the row is mounted.
    pub fn set_text(&self, text: &str) {
        unsafe {
            let ns = NSString::from_str(text);
            let _: () = msg_send![&*self.field, setText: &*ns];
        }
    }

    /// Current text (or "" when nil).
    pub fn text(&self) -> String {
        unsafe {
            let ns_ptr: *mut NSString = msg_send![&*self.field, text];
            if ns_ptr.is_null() {
                String::new()
            } else {
                let ns: &NSString = &*ns_ptr;
                ns.to_string()
            }
        }
    }
}

/// Configuration knobs for [`make_text_field`].
#[derive(Clone, Copy, Default)]
pub struct TextFieldConfig {
    /// `isSecureTextEntry` — set by [`make_secure_text_field`].
    pub secure: bool,
    /// `UIKeyboardType` raw value: 0 = default, 4 = numberPad. Matches
    /// the SwiftUI `.keyboard` modifier passed by `ShadcnTextField`.
    pub keyboard_type: i64,
    /// `UITextAutocapitalizationType` raw value: 0 = none.
    pub autocapitalization: i64,
    /// `autocorrectionType`: -1 = default, 1 = no.
    pub autocorrection: i64,
}

/// Build a labelled text-field row.
pub fn make_text_field(
    mtm: MainThreadMarker,
    label: &str,
    placeholder: &str,
    config: TextFieldConfig,
) -> (Retained<UIView>, TextFieldHandle) {
    // Field label — sentence-case 13 pt medium `ShadcnPrimary`, mirroring
    // SwiftUI `ShadcnField`'s `.footnote.weight(.medium)` text style with
    // `Color("ShadcnPrimary")` foreground.
    let label_view = field_label(mtm, label);

    // The field itself.
    let field: Retained<UITextField> = unsafe { msg_send![UITextField::class(), new] };
    unsafe {
        let ns_placeholder = NSString::from_str(placeholder);
        let _: () = msg_send![&*field, setPlaceholder: &*ns_placeholder];
        // SwiftUI `ShadcnTextField` uses `.font(.callout)` (16 pt regular).
        let _: () =
            msg_send![&*field, setFont: &*typography::system(16.0, typography::WEIGHT_REGULAR)];
        let _: () = msg_send![&*field, setTextColor: &*colors::shadcn_primary()];
        // borderStyle: UITextBorderStyleNone = 0 (we draw our own).
        let _: () = msg_send![&*field, setBorderStyle: 0_i64];
        let _: () = msg_send![&*field, setSecureTextEntry: config.secure];
        let _: () = msg_send![&*field, setKeyboardType: config.keyboard_type];
        let _: () = msg_send![&*field, setAutocapitalizationType: config.autocapitalization];
        let _: () = msg_send![&*field, setAutocorrectionType: config.autocorrection];

        // Layer chrome.
        let layer: Retained<AnyObject> = msg_send![&*field, layer];
        let _: () = msg_send![&*layer, setCornerRadius: TextFieldMetrics::CORNER_RADIUS];
        let _: () = msg_send![&*layer, setBorderWidth: TextFieldMetrics::BORDER_WIDTH];
        let _: () = msg_send![&*layer, setMasksToBounds: true];
        let border = colors::shadcn_input();
        let cg: *const AnyObject = msg_send![&*border, CGColor];
        let _: () = msg_send![&*layer, setBorderColor: cg];

        // Intrinsic 36 pt height — pin via height constraint so the
        // surrounding vertical stack reserves the right space.
        let height: Retained<AnyObject> = msg_send![&*field, heightAnchor];
        let constraint: Retained<AnyObject> =
            msg_send![&*height, constraintEqualToConstant: TextFieldMetrics::FIELD_HEIGHT];
        let _: () = msg_send![&*constraint, setActive: true];

        // Inset the text inside the field — UITextField has no padding
        // property, but a left/right view with width-set frame is the
        // canonical workaround.
        let make_pad: extern "C" fn() -> *mut UIView = {
            // Use a small dedicated UIView with intrinsic width.
            // Reaching for the runtime via msg_send keeps us off the
            // UIView::initWithFrame signature dance.
            extern "C" fn placeholder() -> *mut UIView {
                std::ptr::null_mut()
            }
            placeholder
        };
        let _ = make_pad;
        let pad_left = UIView::new(mtm);
        let pad_right = UIView::new(mtm);
        // Width-pin both padding views.
        for pad in [&pad_left, &pad_right] {
            let w: Retained<AnyObject> = msg_send![&**pad, widthAnchor];
            let k: Retained<AnyObject> =
                msg_send![&*w, constraintEqualToConstant: TextFieldMetrics::HORIZONTAL_PADDING];
            let _: () = msg_send![&*k, setActive: true];
        }
        let _: () = msg_send![&*field, setLeftView: &*pad_left];
        let _: () = msg_send![&*field, setLeftViewMode: 3_i64]; // always
        let _: () = msg_send![&*field, setRightView: &*pad_right];
        let _: () = msg_send![&*field, setRightViewMode: 3_i64];
    }

    // Vertical wrapper.
    let stack = UIStackView::new(mtm);
    stack.setAxis(UILayoutConstraintAxis::Vertical);
    stack.setAlignment(UIStackViewAlignment::Fill);
    stack.setDistribution(UIStackViewDistribution::Fill);
    stack.setSpacing(TextFieldMetrics::LABEL_TO_FIELD_SPACING);
    stack.addArrangedSubview(&label_view);
    stack.addArrangedSubview(unsafe { &*(&*field as *const UITextField as *const UIView) });

    let row_view: Retained<UIView> = unsafe { Retained::cast_unchecked::<UIView>(stack) };
    let handle = TextFieldHandle {
        field: field.clone(),
    };
    (row_view, handle)
}
