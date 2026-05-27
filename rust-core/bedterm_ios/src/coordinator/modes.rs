//! Composer open / close — State1 ↔ State3 transitions.

use super::{BtIosKeyboardCoordinator, FocusTarget};
use objc2::msg_send;
use objc2::DefinedClass;

impl BtIosKeyboardCoordinator {
    pub(super) fn do_open_composer(&self) {
        let ms_ref = self.ivars().mode_state.borrow();
        let Some(ms) = ms_ref.as_ref().cloned() else {
            return;
        };
        drop(ms_ref);
        ms.composer_tapped();
        // Focusing happens in VC's modeStateDidChange handler so that
        // canBecomeFirstResponder flips first.
    }

    pub(super) fn do_close_composer(&self) {
        let v2 = self.ivars().view2.get();
        let composer_is_first = if v2.is_null() {
            false
        } else {
            let f: bool = unsafe { msg_send![v2, isFirstResponder] };
            f
        };
        if !v2.is_null() {
            let _: bool = unsafe { msg_send![v2, resignFirstResponder] };
        }
        // Flip mode → State2 (this also re-enables view1's first
        // responder eligibility via modeStateDidChange).
        let ms_ref = self.ivars().mode_state.borrow();
        let Some(ms) = ms_ref.as_ref().cloned() else {
            return;
        };
        drop(ms_ref);
        ms.close_composer_tapped();
        if composer_is_first {
            self.do_focus_view1_force();
        }
        self.ivars().focused.set(FocusTarget::None as u8);
    }
}
