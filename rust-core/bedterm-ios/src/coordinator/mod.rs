//! Centralised keyboard / first-responder router.
//!
//! Both `view1` (BtIosMetalInputView) and `view2` (UITextView) report focus
//! transitions through this object instead of talking to each other. Future
//! keyboard logic (custom accessory views, modifier tracking, key-command
//! routing) goes here.
//!
//! The implementation is split across sub-modules along its responsibilities:
//!
//! - [`focus`]   — first-responder routing between view1 / view2.
//! - [`chips`]   — keybar chips (tab / newline / esc / ctrl / send) and the
//!   Ctrl latch state machine.
//! - [`dpad`]    — arrow-key chips (CSI cursor sequences).
//! - [`modes`]   — composer open / close (State1 ↔ State3 transitions).
//! - [`emit`]    — shared byte-emission path used by chips & dpad.
//!
//! Selectors live on the class in this file as one-line forwarders into the
//! per-concern helpers.

use crate::input_mode::ModeState;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_foundation::NSString;
use objc2_ui_kit::UIButton;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod chips;
mod dpad;
mod emit;
mod focus;
mod modes;

/// Generation counter for the Ctrl auto-unlatch timer. Bumped every time the
/// latch toggles so stale `dispatch_after` blocks become no-ops.
pub(super) type CtrlGen = u64;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    None = 0,
    View1 = 1,
    View2 = 2,
}

#[derive(Default)]
pub struct Ivars {
    /// Weak ref to view1 (`BtIosMetalInputView`). UIKit retains it through
    /// the VC's view hierarchy, and the VC's `Ivars::view1` holds a second
    /// strong ref. This coordinator is itself owned by the VC, so the VC
    /// (and therefore view1) outlive every read of this pointer. Set once
    /// in `BtIosKeyboardCoordinator::new` from inside `viewDidLoad`,
    /// never cleared.
    ///
    /// SAFETY: only deref when `self.view1.get()` is non-null AND we are
    /// on the main thread (guaranteed by `MainThreadOnly`). Producers run
    /// in `viewDidLoad`; consumers run from selectors / FFI which are
    /// already main-thread.
    pub(super) view1: Cell<*const AnyObject>,
    /// Weak ref to view2 (`UITextView`). Same lifecycle as `view1`: UIKit
    /// retains via the subview tree, VC retains a second strong ref in
    /// `Ivars::view2`. Set in `BtIosKeyboardCoordinator::new`.
    ///
    /// SAFETY: identical contract to `view1` — non-null + main-thread.
    pub(super) view2: Cell<*const AnyObject>,
    /// Currently focused target.
    pub(super) focused: Cell<u8>,
    /// Most recent text2 contents — used to compute view1's bg color the next
    /// time view1 takes focus.
    pub(super) last_text2: RefCell<String>,
    /// Bg color for view1 derived from `last_text2`. Recomputed in
    /// `notifyText2Changed:` and applied in `focusView1`.
    pub(super) pending_color: Cell<(f32, f32, f32, f32)>,
    /// Keybar Ctrl latch — toggles the Ctrl button tint and mirrors the
    /// flag onto view1 so its `insertText:` XOR-masks the next ASCII
    /// letter byte (Swift `KeyBarState.ctrlPending` parity).
    pub(super) ctrl_pending: Cell<bool>,
    /// Retained ref to the Ctrl chip button so we can re-tint it when the
    /// latch toggles.
    pub(super) ctrl_button: RefCell<Option<Retained<UIButton>>>,
    /// Mode state shared with VC / HUD / metal_view. `Rc` — main-thread only.
    pub(super) mode_state: RefCell<Option<Rc<ModeState>>>,
    /// Generation counter for the Ctrl auto-unlatch dispatch_after. Each
    /// toggle bumps it; a stale fired closure compares + bails.
    pub(super) ctrl_gen: Cell<CtrlGen>,
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

        /// Keybar: Tab — emits `0x09` to the PTY and clears the Ctrl latch.
        #[unsafe(method(keybarTab))]
        fn keybar_tab(&self) {
            self.do_keybar_tab();
        }

        /// Keybar: Newline — emits CR (`0x0D`) to the PTY.
        #[unsafe(method(keybarNewline))]
        fn keybar_newline(&self) {
            self.do_keybar_newline();
        }

        /// Keybar: Esc — emits `0x1B` and clears the Ctrl latch.
        #[unsafe(method(keybarEsc))]
        fn keybar_esc(&self) {
            self.do_keybar_esc();
        }

        /// Dpad: Up — emits CSI ESC `[A` (VT100 cursor-up).
        #[unsafe(method(dpadUp:))]
        fn dpad_up(&self, _sender: &AnyObject) {
            self.do_dpad_up();
        }

        /// Dpad: Down — emits CSI ESC `[B`.
        #[unsafe(method(dpadDown:))]
        fn dpad_down(&self, _sender: &AnyObject) {
            self.do_dpad_down();
        }

        /// Dpad: Right — emits CSI ESC `[C`.
        #[unsafe(method(dpadRight:))]
        fn dpad_right(&self, _sender: &AnyObject) {
            self.do_dpad_right();
        }

        /// Dpad: Left — emits CSI ESC `[D`.
        #[unsafe(method(dpadLeft:))]
        fn dpad_left(&self, _sender: &AnyObject) {
            self.do_dpad_left();
        }

        /// Keybar: Ctrl — toggles the latch + re-tints the bar button.
        #[unsafe(method(keybarCtrl))]
        fn keybar_ctrl(&self) {
            self.do_keybar_ctrl();
        }

        /// Called by the metal view when its `insert_text` consumed the
        /// Ctrl latch internally — clears the visual tint to stay in sync.
        #[unsafe(method(ctrlLatchConsumed))]
        fn ctrl_latch_consumed(&self) {
            self.do_ctrl_latch_consumed();
        }

        /// Disconnect banner — "Reconnect" button tap. Stub.
        #[unsafe(method(disconnectBannerReconnect:))]
        fn disconnect_banner_reconnect(&self, _sender: &AnyObject) {
            eprintln!("[bedterm_ios] disconnectBannerReconnect: tapped");
        }

        /// Connecting-overlay "Cancel" button tap. Stub.
        #[unsafe(method(connectingOverlayCancel:))]
        fn connecting_overlay_cancel(&self, _sender: &AnyObject) {
            eprintln!("[bedterm_ios] connectingOverlayCancel: tapped");
        }

        /// State1 send chip — reads view2's buffer, forwards through the
        /// metal view's PTY sink, then clears the composer.
        #[unsafe(method(keybarSend:))]
        fn keybar_send(&self, _sender: &AnyObject) {
            self.do_keybar_send();
        }

        /// Composer chip — promotes to State3 then focuses view2.
        #[unsafe(method(openComposer:))]
        fn open_composer(&self, _sender: &AnyObject) {
            self.do_open_composer();
        }

        /// Close-composer chip — if composer is firstResponder, move focus
        /// to view1; otherwise just resign.
        #[unsafe(method(closeComposer:))]
        fn close_composer(&self, _sender: &AnyObject) {
            self.do_close_composer();
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
}
