//! Keybar chips (Tab / Newline / Esc / Ctrl / Send) and Ctrl latch state.
//!
//! The Ctrl latch mirrors Swift's `KeyBarState.ctrlPending`:
//! - Tapping Ctrl toggles the latch, re-tints the button, mirrors the flag
//!   onto view1, and schedules a 3 s auto-unlatch.
//! - Any non-character chip (Tab / Newline / Esc / Dpad / Send) clears the
//!   latch via `clear_ctrl_latch`.
//! - When view1 consumes the latch internally (XOR-masking the next ASCII
//!   letter in `insertText:`), it calls `ctrlLatchConsumed` on the
//!   coordinator so the visual tint stays in sync.

use super::{BtIosKeyboardCoordinator, CtrlGen};
use crate::metal_view::BtIosMetalInputView;
use objc2::rc::Retained;
use objc2::{msg_send, ClassType, DefinedClass};
use objc2_foundation::NSString;
use objc2_ui_kit::UIColor;

impl BtIosKeyboardCoordinator {
    pub(super) fn do_keybar_tab(&self) {
        self.clear_ctrl_latch();
        self.emit_via_view(&[0x09]);
    }

    pub(super) fn do_keybar_newline(&self) {
        self.clear_ctrl_latch();
        self.emit_via_view(&[0x0D]);
    }

    pub(super) fn do_keybar_esc(&self) {
        self.clear_ctrl_latch();
        self.emit_via_view(&[0x1B]);
    }

    pub(super) fn do_keybar_ctrl(&self) {
        let next = !self.ivars().ctrl_pending.get();
        self.ivars().ctrl_pending.set(next);
        self.apply_ctrl_tint(next);
        self.sync_view_ctrl(next);
        let gen = self.ivars().ctrl_gen.get().wrapping_add(1);
        self.ivars().ctrl_gen.set(gen);
        if next {
            self.schedule_ctrl_unlatch(gen);
        }
    }

    pub(super) fn do_ctrl_latch_consumed(&self) {
        if !self.ivars().ctrl_pending.get() {
            return;
        }
        self.ivars().ctrl_pending.set(false);
        self.apply_ctrl_tint(false);
        // Bump the generation so the pending dispatch_after no-ops.
        let gen = self.ivars().ctrl_gen.get().wrapping_add(1);
        self.ivars().ctrl_gen.set(gen);
    }

    pub(super) fn do_keybar_send(&self) {
        let v2 = self.ivars().view2.get();
        if v2.is_null() {
            return;
        }
        let text: Option<Retained<NSString>> = unsafe { msg_send![v2, text] };
        let s = text.map(|t| t.to_string()).unwrap_or_default();
        if s.is_empty() {
            return;
        }
        // Emit buffer + newline (separate calls — keeps the trailing
        // CR distinct so the sink can spot the boundary).
        self.emit_via_view(s.as_bytes());
        self.emit_via_view(b"\n");
        let empty = NSString::from_str("");
        let _: () = unsafe { msg_send![v2, setText: &*empty] };
    }

    /// Clear the Ctrl latch (visual + view-side). Mirrors Swift's
    /// `KeyBarState.consumeCtrlIfPending` for non-character chips.
    pub(super) fn clear_ctrl_latch(&self) {
        if !self.ivars().ctrl_pending.get() {
            return;
        }
        self.ivars().ctrl_pending.set(false);
        self.apply_ctrl_tint(false);
        self.sync_view_ctrl(false);
        // Bump generation so the pending auto-unlatch becomes a no-op.
        let gen = self.ivars().ctrl_gen.get().wrapping_add(1);
        self.ivars().ctrl_gen.set(gen);
    }

    /// Mirror the latch state onto view1 so its `insert_text` knows to
    /// XOR-mask the next character. No-op when the view is detached.
    pub(super) fn sync_view_ctrl(&self, pending: bool) {
        let v1 = self.ivars().view1.get();
        if v1.is_null() {
            return;
        }
        let view = unsafe { &*(v1 as *const BtIosMetalInputView) };
        view.set_ctrl_pending(pending);
    }

    /// Schedule a 3 s auto-unlatch on the main queue. The closure compares
    /// the captured generation to the live one; if they don't match the
    /// latch was already toggled / consumed and the fire is a no-op.
    pub(super) fn schedule_ctrl_unlatch(&self, gen: CtrlGen) {
        let self_ptr: *const Self = self;
        let payload = Box::into_raw(Box::new((self_ptr, gen)));
        extern "C" {
            fn dispatch_after_f(
                when: u64,
                queue: *mut std::ffi::c_void,
                ctx: *mut std::ffi::c_void,
                work: unsafe extern "C" fn(*mut std::ffi::c_void),
            );
            fn dispatch_time(when: u64, delta: i64) -> u64;
            static _dispatch_main_q: std::ffi::c_void;
        }
        const DISPATCH_TIME_NOW: u64 = 0;
        const NSEC_PER_SEC: i64 = 1_000_000_000;
        unsafe extern "C" fn fire(ctx: *mut std::ffi::c_void) {
            // SAFETY: box was leaked in `schedule_ctrl_unlatch`; recover here.
            let boxed =
                unsafe { Box::from_raw(ctx as *mut (*const BtIosKeyboardCoordinator, CtrlGen)) };
            let (self_ptr, captured_gen) = *boxed;
            // SAFETY: coordinator is retained by the VC for its lifetime;
            // the main queue runs on the same thread that holds it. The
            // generation compare guards against use-after-free in any
            // practical reorder.
            let coord = unsafe { &*self_ptr };
            if coord.ivars().ctrl_gen.get() != captured_gen {
                return;
            }
            if !coord.ivars().ctrl_pending.get() {
                return;
            }
            coord.ivars().ctrl_pending.set(false);
            coord.apply_ctrl_tint(false);
            coord.sync_view_ctrl(false);
        }
        unsafe {
            let when = dispatch_time(DISPATCH_TIME_NOW, 3 * NSEC_PER_SEC);
            dispatch_after_f(
                when,
                &_dispatch_main_q as *const _ as *mut std::ffi::c_void,
                payload as *mut std::ffi::c_void,
                fire,
            );
        }
    }

    pub(super) fn apply_ctrl_tint(&self, pending: bool) {
        let button_ref = self.ivars().ctrl_button.borrow();
        let Some(button) = button_ref.as_ref() else {
            return;
        };
        // Re-fetch the existing configuration, swap the foreground colour, and
        // write it back — UIButtonConfiguration is value-semantic.
        let cfg = button.configuration();
        let Some(cfg) = cfg else { return };
        let colour: Retained<UIColor> = if pending {
            unsafe { msg_send![UIColor::class(), systemBlueColor] }
        } else {
            unsafe { msg_send![UIColor::class(), secondaryLabelColor] }
        };
        cfg.setBaseForegroundColor(Some(&colour));
        button.setConfiguration(Some(&cfg));
    }
}
