//! Centralised accessibility-identifier helper.
//!
//! UI tests (BedTermUITests/RustTerminalSmokeUITests) need stable a11y IDs
//! to address subviews built by Rust. Each `setAccessibilityIdentifier:`
//! call site is identical boilerplate — wrap it in one place.

use objc2::msg_send;
use objc2::runtime::AnyObject;
use objc2_foundation::NSString;

/// Set `accessibilityIdentifier` on any `UIView` / `UIButton` / etc.
///
/// # Safety
/// `view` must point at a live ObjC object that responds to
/// `setAccessibilityIdentifier:` (any UIView subclass does).
pub(crate) fn set_a11y_id(view: &AnyObject, identifier: &str) {
    let ns = NSString::from_str(identifier);
    let _: () = unsafe { msg_send![view, setAccessibilityIdentifier: &*ns] };
}
