//! iOS-specific pasteboard integration for block-list text selection.
//! Pure types (`BlockSelectionState`, `BlockHit`, `block_hit_test`, etc.)
//! are re-exported from `bedterm_app::block_list::selection`.

#![allow(dead_code)]

// Re-export pure types from bedterm-app
pub use bedterm_app::block_list::selection::{
    block_hit_test, ActiveSelection, BlockHit, BlockSelectionState,
};

#[cfg(target_os = "ios")]
use objc2_foundation::NSString;
#[cfg(target_os = "ios")]
use objc2_ui_kit::UIPasteboard;

/// Copy a string to `UIPasteboard.generalPasteboard`. Mirrors Swift's
/// one-liner: just the selected text, nothing prepended.
#[cfg(target_os = "ios")]
pub fn copy_to_pasteboard(text: &str) {
    if text.is_empty() {
        return;
    }
    let s = NSString::from_str(text);
    let pb = UIPasteboard::generalPasteboard();
    unsafe { pb.setString(Some(&s)) };
}
