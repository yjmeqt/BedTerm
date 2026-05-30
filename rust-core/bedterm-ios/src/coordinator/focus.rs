//! First-responder routing between view1 (metal) and view2 (composer).
//!
//! Focus is gated on the active `InputMode`:
//! - view1 may only become first responder in State2.
//! - view2 may become first responder in State1 / State3 but not State2.
//!
//! `do_focus_view1_force` skips the gate — used by `closeComposer:` after the
//! mode has just flipped to State2.

use super::{BtIosKeyboardCoordinator, FocusTarget};
use bedterm_app::input_mode::InputMode;
use objc2::msg_send;
use objc2::DefinedClass;

impl BtIosKeyboardCoordinator {
    pub(super) fn do_focus_view1(&self) {
        // Gate on mode: view1 may only become first responder in State2.
        if let Some(ms) = self.ivars().mode_state.borrow().as_ref() {
            if ms.mode() != InputMode::State2 {
                return;
            }
        }
        self.do_focus_view1_force();
    }

    /// Mode-bypassing variant — used by `closeComposer:` to move focus
    /// to view1 after the mode has just flipped to State2.
    pub(super) fn do_focus_view1_force(&self) {
        let v1 = self.ivars().view1.get();
        if v1.is_null() {
            return;
        }
        let _: bool = unsafe { msg_send![v1, becomeFirstResponder] };
        self.ivars().focused.set(FocusTarget::View1 as u8);
    }

    pub(super) fn do_focus_view2(&self) {
        // view2 is the shared composer — valid in State1 and State3,
        // but not in State2 (where it's hidden).
        if let Some(ms) = self.ivars().mode_state.borrow().as_ref() {
            if ms.mode() == InputMode::State2 {
                return;
            }
        }
        let v2 = self.ivars().view2.get();
        if v2.is_null() {
            return;
        }
        let _: bool = unsafe { msg_send![v2, becomeFirstResponder] };
        self.ivars().focused.set(FocusTarget::View2 as u8);
    }
}
