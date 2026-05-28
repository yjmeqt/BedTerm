//! Form row with a `UISwitch` as the trailing accessory.

use crate::design_system::components::form_row;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, ClassType};
use objc2_foundation::MainThreadMarker;
use objc2_ui_kit::{UISwitch, UIView};

/// `UIControlEventValueChanged` = 1 << 12.
const CONTROL_EVENT_VALUE_CHANGED: u64 = 1 << 12;

/// Build a row showing `label` on the leading edge and a UISwitch on the
/// trailing edge. `target`/`action` receive `valueChanged:` events.
pub fn toggle_row(
    mtm: MainThreadMarker,
    label: &str,
    on: bool,
    target: &AnyObject,
    action: Sel,
) -> (Retained<UIView>, Retained<UISwitch>) {
    let toggle: Retained<UISwitch> = unsafe { msg_send![UISwitch::class(), new] };
    toggle.setOn(on);
    let _: () = unsafe {
        msg_send![&*toggle,
            addTarget: target,
            action: action,
            forControlEvents: CONTROL_EVENT_VALUE_CHANGED,
        ]
    };

    // `UISwitch: UIControl: UIView` — Deref chain covers the upcast.
    let row = form_row(mtm, label, &toggle);
    (row, toggle)
}
