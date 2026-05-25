//! Centralised keyboard / first-responder router.
//!
//! Both `view1` (BtIosMetalInputView) and `view2` (UITextView) report focus
//! transitions through this object instead of talking to each other. Future
//! keyboard logic (custom accessory views, modifier tracking, key-command
//! routing) goes here.

use crate::input_mode::{InputMode, ModeState};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, ClassType, DefinedClass, MainThreadOnly};
use objc2_foundation::NSString;
use objc2_ui_kit::{UIButton, UIColor};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

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
    /// Keybar Ctrl latch — when true, the next char typed into view2 would
    /// (in a real PTY world) be Ctrl-modified. For now it just drives the
    /// Ctrl button's tint.
    ctrl_pending: Cell<bool>,
    /// Retained ref to the Ctrl chip button so we can re-tint it when the
    /// latch toggles.
    ctrl_button: RefCell<Option<Retained<UIButton>>>,
    /// Mode state shared with VC / HUD / metal_view. `Rc` — main-thread only.
    mode_state: RefCell<Option<Rc<ModeState>>>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosKeyboardCoordinator"]
    #[ivars = Ivars]
    pub struct BtIosKeyboardCoordinator;

    impl BtIosKeyboardCoordinator {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send![super(this), init] }
        }

        /// Make view1 the first responder and apply the pending background color.
        #[unsafe(method(focusView1))]
        fn focus_view1_obj(&self) {
            self.do_focus_view1();
        }

        /// Make view2 the first responder.
        #[unsafe(method(focusView2))]
        fn focus_view2_obj(&self) {
            self.do_focus_view2();
        }

        /// Returns the current `FocusTarget` discriminant.
        #[unsafe(method(currentFocus))]
        fn current_focus(&self) -> u8 {
            self.ivars().focused.get()
        }

        /// Called by the VC when view2's text changes. Reserved as a hook for
        /// future keyboard logic — view1's colour is now driven by its own
        /// text buffer, updated in `insertText:`/`deleteBackward:`.
        #[unsafe(method(notifyText2Changed:))]
        fn notify_text2_changed(&self, text: &NSString) {
            *self.ivars().last_text2.borrow_mut() = text.to_string();
        }

        /// Tap recogniser target for view1. Routes through `focusView1`.
        #[unsafe(method(handleView1Tap:))]
        fn handle_view1_tap(&self, _sender: &AnyObject) {
            self.do_focus_view1();
        }

        /// Keybar: Tab — inserts a literal `\t` into view2 at the cursor.
        #[unsafe(method(keybarTab))]
        fn keybar_tab(&self) {
            self.insert_into_view2("\t");
        }

        /// Keybar: Newline — inserts `\n` into view2 at the cursor.
        #[unsafe(method(keybarNewline))]
        fn keybar_newline(&self) {
            self.insert_into_view2("\n");
        }

        /// Keybar: Esc — placeholder until a PTY is wired up.
        #[unsafe(method(keybarEsc))]
        fn keybar_esc(&self) {
            // No-op for now; would send ESC (0x1B) once a PTY exists.
        }

        /// Keybar: Ctrl — toggles the latch and re-tints the bar button.
        #[unsafe(method(keybarCtrl))]
        fn keybar_ctrl(&self) {
            let next = !self.ivars().ctrl_pending.get();
            self.ivars().ctrl_pending.set(next);
            self.apply_ctrl_tint(next);
        }

        /// State1 send chip — reads view2's buffer, fires the (stubbed)
        /// pipeline, then clears the composer. Real submission lands here
        /// once a PTY is wired up.
        #[unsafe(method(keybarSend:))]
        fn keybar_send(&self, _sender: &AnyObject) {
            let v2 = self.ivars().view2.get();
            if v2.is_null() {
                return;
            }
            let text: Option<Retained<NSString>> = unsafe { msg_send![v2, text] };
            let s = text.map(|t| t.to_string()).unwrap_or_default();
            if s.is_empty() {
                return;
            }
            // Stub: log + echo the submitted text back into the composer
            // so the side effect is visible until a PTY pipeline lands.
            eprintln!("[bedterm-ios-ui] keybarSend: {}", s);
            let echo = NSString::from_str(&format!("[send] {}\n", s));
            let _: () = unsafe { msg_send![v2, setText: &*echo] };
        }

        /// Composer chip — promotes to State3 then focuses view2.
        #[unsafe(method(openComposer:))]
        fn open_composer(&self, _sender: &AnyObject) {
            let ms_ref = self.ivars().mode_state.borrow();
            let Some(ms) = ms_ref.as_ref().cloned() else { return };
            drop(ms_ref);
            ms.composer_tapped();
            // Focusing happens in VC's modeStateDidChange handler so that
            // canBecomeFirstResponder flips first.
        }

        /// Close-composer chip — if composer is firstResponder, move focus
        /// to view1; otherwise just resign.
        #[unsafe(method(closeComposer:))]
        fn close_composer(&self, _sender: &AnyObject) {
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
            let Some(ms) = ms_ref.as_ref().cloned() else { return };
            drop(ms_ref);
            ms.close_composer_tapped();
            if composer_is_first {
                self.do_focus_view1_force();
            }
            self.ivars().focused.set(FocusTarget::None as u8);
        }

    }
);

impl BtIosKeyboardCoordinator {
    pub fn new(
        mtm: objc2::MainThreadMarker,
        view1: *const AnyObject,
        view2: *const AnyObject,
    ) -> Retained<Self> {
        let this: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), init] };
        this.ivars().view1.set(view1);
        this.ivars().view2.set(view2);
        this.ivars().focused.set(FocusTarget::None as u8);
        this.ivars().pending_color.set((0.55, 0.55, 0.55, 1.0));
        this
    }

    fn do_focus_view1(&self) {
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
    fn do_focus_view1_force(&self) {
        let v1 = self.ivars().view1.get();
        if v1.is_null() {
            return;
        }
        let _: bool = unsafe { msg_send![v1, becomeFirstResponder] };
        self.ivars().focused.set(FocusTarget::View1 as u8);
    }

    fn do_focus_view2(&self) {
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

    /// Install the shared mode state. Called once from the VC.
    pub(crate) fn set_mode_state(&self, ms: &Rc<ModeState>) {
        *self.ivars().mode_state.borrow_mut() = Some(ms.clone());
    }

    /// Stash the Ctrl chip button so the latch can re-tint it.
    pub(crate) fn set_ctrl_button(&self, button: &UIButton) {
        let retained = unsafe {
            Retained::retain(button as *const UIButton as *mut UIButton).expect("non-null button")
        };
        *self.ivars().ctrl_button.borrow_mut() = Some(retained);
    }

    /// Send `insertText:` to view2 — UITextView is a UIKeyInput conformer, so
    /// this routes through its normal text-entry path (cursor moves, delegate
    /// fires).
    fn insert_into_view2(&self, s: &str) {
        let v2 = self.ivars().view2.get();
        if v2.is_null() {
            return;
        }
        let ns = NSString::from_str(s);
        let _: () = unsafe { msg_send![v2, insertText: &*ns] };
    }

    fn apply_ctrl_tint(&self, pending: bool) {
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
