//! Shared byte-emission path used by chips & dpad.
//!
//! All keybar chips funnel through `emit_via_view`, which forwards bytes
//! through view1's PTY sink (`on_send`). When view1 is detached or no sink
//! is installed yet the call is a silent no-op.

use super::BtIosKeyboardCoordinator;
use crate::metal_view::BtIosMetalInputView;
use objc2::DefinedClass;

impl BtIosKeyboardCoordinator {
    /// Forward `bytes` through view1's PTY sink (`on_send`). No-op when
    /// the view is detached or no sink is installed yet.
    pub(super) fn emit_via_view(&self, bytes: &[u8]) {
        let v1 = self.ivars().view1.get();
        if v1.is_null() {
            return;
        }
        let view = unsafe { &*(v1 as *const BtIosMetalInputView) };
        view.emit(bytes);
    }
}
