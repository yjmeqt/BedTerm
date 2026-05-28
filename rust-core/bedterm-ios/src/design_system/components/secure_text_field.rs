//! Convenience wrapper around [`make_text_field`] with `isSecureTextEntry`
//! forced on. Same shape so the connect-form VC can swap between the
//! password / passphrase field and a plain field without branching the
//! layout code.

use crate::design_system::components::text_field::{
    make_text_field, TextFieldConfig, TextFieldHandle,
};
use objc2::rc::Retained;
use objc2_foundation::MainThreadMarker;
use objc2_ui_kit::UIView;

/// Build a labelled secure text-field row (e.g. password / passphrase).
pub fn make_secure_text_field(
    mtm: MainThreadMarker,
    label: &str,
    placeholder: &str,
) -> (Retained<UIView>, TextFieldHandle) {
    make_text_field(
        mtm,
        label,
        placeholder,
        TextFieldConfig {
            secure: true,
            keyboard_type: 0,
            autocapitalization: 0,
            autocorrection: 1,
        },
    )
}
