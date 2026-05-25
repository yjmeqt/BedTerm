//! Keybar accessory view for view2.
//!
//! A `UIToolbar` containing four `UIBarButtonItem`s — Tab, Newline, Esc, Ctrl —
//! matching the Swift `BlockListComposer` footer order. Attached as
//! `view2.inputAccessoryView` so it tracks the keyboard above view2 whenever
//! view2 is the first responder.
//!
//! All four items target the shared `BtRsKeyboardCoordinator`; the action
//! methods live on the coordinator (see `coordinator.rs`).

use crate::coordinator::BtRsKeyboardCoordinator;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, msg_send_id, sel};
use objc2_foundation::{MainThreadMarker, NSArray, NSString};
use objc2_ui_kit::{
    UIBarButtonItem, UIBarButtonItemStyle, UIBarButtonSystemItem, UIImage, UIToolbar,
};

/// SF Symbol names — same glyphs as the Swift composer footer.
const ICON_TAB: &str = "arrow.right.to.line.compact";
const ICON_NEWLINE: &str = "return";
const ICON_ESC: &str = "escape";
const ICON_CTRL: &str = "control";

pub(crate) fn make_keybar(
    mtm: MainThreadMarker,
    coordinator: &BtRsKeyboardCoordinator,
) -> Retained<UIToolbar> {
    let toolbar: Retained<UIToolbar> = unsafe { msg_send_id![mtm.alloc::<UIToolbar>(), init] };
    let _: () = unsafe { msg_send![&*toolbar, sizeToFit] };

    let target: &AnyObject = &*coordinator as &AnyObject;

    let tab = bar_button(mtm, ICON_TAB, target, sel!(keybarTab));
    let newline = bar_button(mtm, ICON_NEWLINE, target, sel!(keybarNewline));
    let esc = bar_button(mtm, ICON_ESC, target, sel!(keybarEsc));
    let ctrl = bar_button(mtm, ICON_CTRL, target, sel!(keybarCtrl));
    let flex = flexible_space(mtm);

    // Layout: Tab · Newline · Esc · Ctrl · <flex>
    let items_array: Retained<NSArray<UIBarButtonItem>> =
        NSArray::from_vec(vec![tab, newline, esc, ctrl.clone(), flex]);
    unsafe { toolbar.setItems(Some(&items_array)) };

    // Hand the Ctrl item back to the coordinator so its latch state can
    // re-tint the button.
    coordinator.set_ctrl_item(&ctrl);

    toolbar
}

fn bar_button(
    mtm: MainThreadMarker,
    icon: &str,
    target: &AnyObject,
    action: objc2::runtime::Sel,
) -> Retained<UIBarButtonItem> {
    let name = NSString::from_str(icon);
    let image = unsafe { UIImage::systemImageNamed(&name) };
    unsafe {
        UIBarButtonItem::initWithImage_style_target_action(
            mtm.alloc::<UIBarButtonItem>(),
            image.as_deref(),
            UIBarButtonItemStyle::Plain,
            Some(target),
            Some(action),
        )
    }
}

fn flexible_space(mtm: MainThreadMarker) -> Retained<UIBarButtonItem> {
    unsafe {
        UIBarButtonItem::initWithBarButtonSystemItem_target_action(
            mtm.alloc::<UIBarButtonItem>(),
            UIBarButtonSystemItem::FlexibleSpace,
            None,
            None,
        )
    }
}
