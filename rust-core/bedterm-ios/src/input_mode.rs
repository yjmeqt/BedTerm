//! 3-state input subsystem.
//!
//! `InputMode` partitions the terminal's bottom chrome into three exclusive
//! states. Only one is mounted (visible + interactive) at a time.
//!
//! - **State1 — Inline input bar.** Bottom row contains
//!   `[input field] [tab esc ctrl] [dpad kbdToggle?] [send]`. `view1`
//!   (the Metal canvas) CANNOT become first responder. Tapping the input
//!   field focuses it. The software-keyboard Return key inserts `\n`
//!   instead of dismissing. Tab/Esc/Ctrl-modified keys route through the
//!   input field's pipeline (currently stubbed by inserting into `view2`).
//!   Send pushes the inline buffer into the `view1` pipeline (stubbed).
//!
//! - **State2 — No input.** Bottom row contains
//!   `[tab esc ctrl] [dpad kbdToggle?] [composer]`. `view1` CAN become
//!   first responder via tap. Tapping the composer chip promotes to State3
//!   and focuses the composer.
//!
//! - **State3 — Composer.** The composer (`view2`) is visible. The bottom
//!   action row is `[newline tab esc ctrl] [dpad kbdToggle?] [closeComposer]`.
//!   `view1` CANNOT become first responder. Close-composer: if the composer
//!   is first responder, move first responder to `view1`; otherwise resign.
//!
//! kbdToggle is only present when no hardware keyboard is attached. When
//! the software keyboard is up, tapping it resigns first responder.
//!
//! HUD switcher: `input 1` forces State1; `input 2/3` leaves State1
//! (defaulting to State2; a composer tap promotes to State3; closing
//! the composer returns to State2).
//!
//! ## Threading
//!
//! `ModeState` is `!Send + !Sync` — every field, and every UIKit object
//! it touches, is main-thread only. Construct, read, and mutate from
//! the main thread only.

use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
use std::cell::Cell;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // State1 is only constructed by iOS-gated HUD switcher path
pub enum InputMode {
    State1 = 1,
    State2 = 2,
    State3 = 3,
}

/// Central mode + keyboard-state holder shared by VC / coordinator / HUD /
/// metal_view. `!Send !Sync` — main-thread only.
#[allow(dead_code)]
pub struct ModeState {
    mode: Cell<InputMode>,
    sw_kbd_visible: Cell<bool>,
    hw_kbd_attached: Cell<bool>,
    /// Weak raw pointer to the owning `RsTerminalViewController`. Used to
    /// post `modeStateDidChange` (via `performSelector:`) on every state
    /// mutation. The VC stores the authoritative `Rc<ModeState>` in its
    /// `Ivars::mode_state`; clones live on the coordinator, the metal
    /// view, and the HUD. The VC drops its `Rc` only after UIKit has
    /// torn down its subview tree — and the child holders (which hold
    /// `Rc` clones) live as long as the VC — so by the time the last
    /// `Rc` strong count hits zero, the VC has already finished
    /// `dealloc`. We never call back into a dead VC.
    ///
    /// In practice the only edge that could break this is if a
    /// background queue retained the `Rc` clone past the VC's lifetime.
    /// `ModeState` is `!Send + !Sync` to make that a compile error.
    ///
    /// SAFETY: only deref when `self.vc.get()` is non-null AND we are on
    /// the main thread. `notify()` enforces both: the null-check is
    /// explicit, and the surrounding `ModeState` is `!Send + !Sync`.
    vc: Cell<*const AnyObject>,
}

// Explicitly opt out of Send/Sync — we hold `Cell<*const AnyObject>`.
// `Cell` already gives us `!Sync`; raw pointers give us `!Send`. Document
// the intent for readers.
//
// (No `unsafe impl Send/Sync` here.)

#[allow(dead_code)]
impl ModeState {
    /// Build a fresh ModeState in `State2` (the default "no input" mode).
    /// `vc` is a weak raw pointer to the owning view controller; it must
    /// outlive the ModeState (in practice the VC owns the `Rc<ModeState>`).
    pub fn new(vc: *const AnyObject) -> Self {
        Self {
            mode: Cell::new(InputMode::State2),
            sw_kbd_visible: Cell::new(false),
            hw_kbd_attached: Cell::new(false),
            vc: Cell::new(vc),
        }
    }

    pub fn mode(&self) -> InputMode {
        self.mode.get()
    }

    pub fn sw_kbd_visible(&self) -> bool {
        self.sw_kbd_visible.get()
    }

    pub fn hw_kbd_attached(&self) -> bool {
        self.hw_kbd_attached.get()
    }

    /// Update VC pointer (used post-construction since the VC needs to
    /// construct the `Rc<ModeState>` before it can capture `&self`).
    pub fn set_vc(&self, vc: *const AnyObject) {
        self.vc.set(vc);
    }

    pub fn set_mode(&self, next: InputMode) {
        if self.mode.get() == next {
            return;
        }
        self.mode.set(next);
        self.notify();
    }

    /// Composer chip tapped in State2 → State3.
    pub fn composer_tapped(&self) {
        if self.mode.get() != InputMode::State3 {
            self.mode.set(InputMode::State3);
            self.notify();
        }
    }

    /// Close-composer chip tapped in State3 → State2.
    pub fn close_composer_tapped(&self) {
        if self.mode.get() != InputMode::State2 {
            self.mode.set(InputMode::State2);
            self.notify();
        }
    }

    pub fn set_sw_kbd_visible(&self, visible: bool) {
        if self.sw_kbd_visible.get() != visible {
            self.sw_kbd_visible.set(visible);
            self.notify();
        }
    }

    pub fn set_hw_kbd_attached(&self, attached: bool) {
        if self.hw_kbd_attached.get() != attached {
            self.hw_kbd_attached.set(attached);
            self.notify();
        }
    }

    fn notify(&self) {
        let vc = self.vc.get();
        if vc.is_null() {
            return;
        }
        // Post relayout selector on the VC. UIKit selectors are main-thread
        // only — by contract this entire object is main-thread only too.
        unsafe {
            let _: () = msg_send![vc, performSelector: sel!(modeStateDidChange)];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_state2() {
        let m = ModeState::new(std::ptr::null());
        assert_eq!(m.mode(), InputMode::State2);
    }

    #[test]
    fn composer_tap_promotes_to_state3() {
        let m = ModeState::new(std::ptr::null());
        m.composer_tapped();
        assert_eq!(m.mode(), InputMode::State3);
    }

    #[test]
    fn close_composer_returns_to_state2() {
        let m = ModeState::new(std::ptr::null());
        m.composer_tapped();
        m.close_composer_tapped();
        assert_eq!(m.mode(), InputMode::State2);
    }

    #[test]
    fn set_mode_explicit() {
        let m = ModeState::new(std::ptr::null());
        m.set_mode(InputMode::State1);
        assert_eq!(m.mode(), InputMode::State1);
        m.set_mode(InputMode::State2);
        assert_eq!(m.mode(), InputMode::State2);
    }
}
