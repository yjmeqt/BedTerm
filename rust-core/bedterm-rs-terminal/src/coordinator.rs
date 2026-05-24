//! Centralised keyboard / first-responder router.
//!
//! Both `view1` (BtRsMetalInputView) and `view2` (UITextView) report focus
//! transitions through this object instead of talking to each other. Future
//! keyboard logic (custom accessory views, modifier tracking, key-command
//! routing) goes here.

use crate::color::hash_to_rgba;
use crate::metal_view::BtRsMetalInputView;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{declare_class, msg_send, msg_send_id, ClassType, DeclaredClass};
use objc2_foundation::NSString;
use std::cell::{Cell, RefCell};

extern "C" {
    fn NSLog(fmt: *const NSString, ...);
}

fn objc2_log(s: &str) {
    let msg = NSString::from_str(s);
    let fmt = NSString::from_str("%@");
    unsafe { NSLog(&*fmt as *const NSString, &*msg as *const NSString) };
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    None = 0,
    View1 = 1,
    View2 = 2,
}

#[derive(Default)]
pub struct Ivars {
    /// Weak ref to view1. Owned by the VC via its subviews; we just observe.
    view1: Cell<*const AnyObject>,
    /// Weak ref to view2. Same lifecycle.
    view2: Cell<*const AnyObject>,
    /// Currently focused target.
    focused: Cell<u8>,
    /// Most recent text2 contents — used to compute view1's bg color the next
    /// time view1 takes focus.
    last_text2: RefCell<String>,
    /// Bg color for view1 derived from `last_text2`. Recomputed in
    /// `notifyText2Changed:` and applied in `focusView1`.
    pending_color: Cell<(f32, f32, f32, f32)>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

declare_class!(
    pub struct BtRsKeyboardCoordinator;

    unsafe impl ClassType for BtRsKeyboardCoordinator {
        type Super = NSObject;
        type Mutability = objc2::mutability::MainThreadOnly;
        const NAME: &'static str = "BtRsKeyboardCoordinator";
    }

    impl DeclaredClass for BtRsKeyboardCoordinator {
        type Ivars = Ivars;
    }

    unsafe impl BtRsKeyboardCoordinator {
        #[method_id(init)]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send_id![super(this), init] }
        }

        /// Make view1 the first responder and apply the pending background color.
        #[method(focusView1)]
        fn focus_view1_obj(&self) {
            self.do_focus_view1();
        }

        /// Make view2 the first responder.
        #[method(focusView2)]
        fn focus_view2_obj(&self) {
            self.do_focus_view2();
        }

        /// Returns the current `FocusTarget` discriminant.
        #[method(currentFocus)]
        fn current_focus(&self) -> u8 {
            self.ivars().focused.get()
        }

        /// Called by the VC when view2's text changes. Stashes the latest text
        /// and pre-computes the next bg color so `focusView1` can apply it
        /// instantly on the next tap.
        #[method(notifyText2Changed:)]
        fn notify_text2_changed(&self, text: &NSString) {
            let s = text.to_string();
            self.ivars().pending_color.set(hash_to_rgba(&s));
            *self.ivars().last_text2.borrow_mut() = s;
        }

        /// Tap recogniser target for view1. Routes through `focusView1`.
        #[method(handleView1Tap:)]
        fn handle_view1_tap(&self, _sender: &AnyObject) {
            objc2_log("[rs-terminal] coordinator.handleView1Tap fired");
            self.do_focus_view1();
        }
    }
);

impl BtRsKeyboardCoordinator {
    pub fn new(
        mtm: objc2_foundation::MainThreadMarker,
        view1: *const AnyObject,
        view2: *const AnyObject,
    ) -> Retained<Self> {
        let this: Retained<Self> = unsafe { msg_send_id![mtm.alloc::<Self>(), init] };
        this.ivars().view1.set(view1);
        this.ivars().view2.set(view2);
        this.ivars().focused.set(FocusTarget::None as u8);
        this.ivars().pending_color.set((0.55, 0.55, 0.55, 1.0));
        this
    }

    fn do_focus_view1(&self) {
        let v1 = self.ivars().view1.get();
        objc2_log(&format!("[rs-terminal] do_focus_view1 v1={:p}", v1));
        if v1.is_null() {
            return;
        }
        let rgba = self.ivars().pending_color.get();
        let view: &BtRsMetalInputView = unsafe { &*(v1 as *const BtRsMetalInputView) };
        view.set_bg_color(rgba);
        let became: bool = unsafe { msg_send![v1, becomeFirstResponder] };
        objc2_log(&format!("[rs-terminal] becomeFirstResponder -> {became}"));
        self.ivars().focused.set(FocusTarget::View1 as u8);
    }

    fn do_focus_view2(&self) {
        let v2 = self.ivars().view2.get();
        if v2.is_null() {
            return;
        }
        let _: bool = unsafe { msg_send![v2, becomeFirstResponder] };
        self.ivars().focused.set(FocusTarget::View2 as u8);
    }
}
