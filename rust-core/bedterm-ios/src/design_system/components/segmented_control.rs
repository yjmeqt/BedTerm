//! UISegmentedControl wrapper used by the W24c connect-form
//! authentication-method picker.
//!
//! Visual rhythm tracks the SwiftUI `ShadcnSegmented`: 34 pt height, 8 pt
//! corner radius, two equal segments. See
//! [`bedterm_app::design_system::segmented_control_metrics::SegmentedControlMetrics`].

use bedterm_app::design_system::segmented_control_metrics::SegmentedControlMetrics;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2_foundation::{MainThreadMarker, NSArray, NSString};
use objc2_ui_kit::UISegmentedControl;

/// `UIControlEventValueChanged` = 1 << 12.
const CONTROL_EVENT_VALUE_CHANGED: u64 = 1 << 12;

/// Build a `UISegmentedControl` populated with `segments` and wired to
/// fire `valueChanged` on `target`/`action`. The control's selected
/// segment can be queried in the selector via
/// `[(UISegmentedControl *)sender selectedSegmentIndex]`.
pub fn make_segmented_control(
    mtm: MainThreadMarker,
    segments: &[&str],
    initial_index: i64,
    target: &AnyObject,
    action: Sel,
) -> Retained<UISegmentedControl> {
    // Build NSArray<NSString*> of titles.
    let ns_titles: Vec<Retained<NSString>> =
        segments.iter().map(|s| NSString::from_str(s)).collect();
    let title_refs: Vec<&NSString> = ns_titles.iter().map(|s| s.as_ref()).collect();
    let array = NSArray::from_slice(&title_refs);

    let control: Retained<UISegmentedControl> = unsafe {
        let alloc = mtm.alloc::<UISegmentedControl>();
        msg_send![alloc, initWithItems: &*array]
    };
    unsafe {
        let _: () = msg_send![&*control, setSelectedSegmentIndex: initial_index];
        let _: () = msg_send![
            &*control,
            addTarget: target,
            action: action,
            forControlEvents: CONTROL_EVENT_VALUE_CHANGED,
        ];

        // Intrinsic 34 pt height.
        let h: Retained<AnyObject> = msg_send![&*control, heightAnchor];
        let k: Retained<AnyObject> =
            msg_send![&*h, constraintEqualToConstant: SegmentedControlMetrics::HEIGHT];
        let _: () = msg_send![&*k, setActive: true];
    }
    let _ = SegmentedControlMetrics::OUTER_CORNER_RADIUS;
    let _ = SegmentedControlMetrics::SELECTION_INSET;
    let _ = mtm;

    control
}
