//! Arrow-key chips — emit VT100 CSI cursor sequences.

use super::BtIosKeyboardCoordinator;

impl BtIosKeyboardCoordinator {
    pub(super) fn do_dpad_up(&self) {
        self.clear_ctrl_latch();
        self.emit_via_view(b"\x1b[A");
    }

    pub(super) fn do_dpad_down(&self) {
        self.clear_ctrl_latch();
        self.emit_via_view(b"\x1b[B");
    }

    pub(super) fn do_dpad_right(&self) {
        self.clear_ctrl_latch();
        self.emit_via_view(b"\x1b[C");
    }

    pub(super) fn do_dpad_left(&self) {
        self.clear_ctrl_latch();
        self.emit_via_view(b"\x1b[D");
    }
}
