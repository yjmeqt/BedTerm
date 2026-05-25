//! The experimental Rust-backed view controller.
//!
//! See [`crate::input_mode`] for the 3-state input subsystem this VC hosts:
//! State1 (inline input bar), State2 (no input, view1 focusable), and
//! State3 (composer). The current state lives in `Ivars.mode_state` and
//! drives subview visibility + first-responder eligibility on every
//! `modeStateDidChange` post.

use crate::action_chip::make_action_chip;
use crate::coordinator::BtIosKeyboardCoordinator;
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize, UIEdgeInsets};
use crate::input_mode::{InputMode, ModeState};
use crate::metal_view::BtIosMetalInputView;
use crate::BtIosBackCallback;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{
    define_class, msg_send, sel, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_foundation::NSString;
use objc2_metal::MTLDevice;
use objc2_ui_kit::{
    UIBarButtonItem, UIBarButtonItemStyle, UIButton, UIColor, UIFont, UINavigationItem,
    UITapGestureRecognizer, UITextView, UIViewController,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

extern "C" {
    /// `MTLCreateSystemDefaultDevice()` — returns the system-preferred GPU.
    fn MTLCreateSystemDefaultDevice() -> *mut ProtocolObject<dyn MTLDevice>;
}

/// Per-instance state stored as ivars.
#[derive(Default)]
pub struct Ivars {
    /// C callback to fire on back-tap (may be None).
    on_back: Cell<Option<BtIosBackCallback>>,
    /// Context pointer for the callback. Only accessed on the main thread.
    ctx: Cell<*mut std::ffi::c_void>,
    /// Upper view — Metal-backed input surface.
    view1: RefCell<Option<Retained<BtIosMetalInputView>>>,
    /// Lower composer text view — sits above the keyboard, 1–3 lines tall.
    view2: RefCell<Option<Retained<UITextView>>>,
    /// Persistent keybar (Tab / Newline / Esc / Ctrl) sandwiched between view1
    /// and view2 — always visible, independent of view2's focus state.
    keybar: RefCell<Option<Retained<objc2_ui_kit::UIView>>>,
    /// State3 keybar variant (with newline chip prepended).
    keybar_with_newline: RefCell<Option<Retained<objc2_ui_kit::UIView>>>,
    /// State1 Run/Send chip — same row as the keybar.
    send_chip: RefCell<Option<Retained<UIButton>>>,
    /// State2 composer chip (icon-only) — same row as the keybar.
    composer_chip: RefCell<Option<Retained<UIButton>>>,
    /// State3 close-composer chip (icon-only) — same row as the keybar.
    close_composer_chip: RefCell<Option<Retained<UIButton>>>,
    /// Tap recognizer on view1 — we toggle its `enabled` flag per mode.
    view1_tap: RefCell<Option<Retained<UITapGestureRecognizer>>>,
    /// Shared 3-state input mode holder (see `input_mode.rs`).
    mode_state: RefCell<Option<Rc<ModeState>>>,
    /// Centralised focus / keyboard router.
    coordinator: RefCell<Option<Retained<BtIosKeyboardCoordinator>>>,
    /// Top-right debug HUD (FPS + hardware keyboard indicator).
    debug_hud: RefCell<Option<Retained<crate::debug_hud::BtIosDebugHUD>>>,
    /// Current dynamic height of view2 (clamped 1–3 lines).
    view2_height: Cell<CGFloat>,
    /// One-line height (font.lineHeight + textContainerInset.top/bottom).
    one_line_height: Cell<CGFloat>,
    /// Three-line ceiling for view2.
    three_line_height: Cell<CGFloat>,
}

// SAFETY: Only accessed on the main thread (MainThreadOnly mutability).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// Experimental Rust-backed terminal view controller.
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosTerminalViewController"]
    #[ivars = Ivars]
    pub struct RsTerminalViewController;

    impl RsTerminalViewController {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send![super(this), init] }
        }

        #[unsafe(method(viewDidLoad))]
        fn view_did_load(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLoad] };

            let mtm = unsafe { MainThreadMarker::new_unchecked() };

            // Background colour — system background so it respects dark mode.
            let bg: Retained<UIColor> =
                unsafe { msg_send![UIColor::class(), systemBackgroundColor] };
            if let Some(view) = self.view() {
                view.setBackgroundColor(Some(&bg));
            }

            // ---------- Back bar button ----------
            let back_title = NSString::from_str("Back");
            let back_btn: Retained<UIBarButtonItem> = unsafe {
                UIBarButtonItem::initWithTitle_style_target_action(
                    mtm.alloc::<UIBarButtonItem>(),
                    Some(&back_title),
                    UIBarButtonItemStyle::Plain,
                    Some(self.as_ref()),
                    Some(sel!(backButtonTapped)),
                )
            };
            let nav_item: Retained<UINavigationItem> =
                unsafe { msg_send![self, navigationItem] };
            nav_item.setLeftBarButtonItem(Some(&back_btn));

            // ---------- Metal device + view1 (Metal canvas) ----------
            let device_ptr = unsafe { MTLCreateSystemDefaultDevice() };
            // Probe: try to instantiate a vanilla MTKView (NOT our subclass).
            // If even this crashes the issue is in MetalKit, not our subclass.
            let view1_opt: Option<Retained<BtIosMetalInputView>> = if device_ptr.is_null() {
                None
            } else {
                let device: Retained<ProtocolObject<dyn MTLDevice>> =
                    unsafe { Retained::from_raw(device_ptr).expect("MTL device") };
                let opt = BtIosMetalInputView::new(mtm, &device);
                if let Some(ref v) = opt {
                    v.set_bg_color((0.55, 0.55, 0.55, 1.0));
                }
                opt
            };

            // ---------- view2 (UITextView composer) ----------
            let zero = CGRect::default();
            let view2: Retained<UITextView> =
                unsafe { msg_send![mtm.alloc::<UITextView>(), initWithFrame: zero] };

            // Make view2 visually distinct so the user can see the boundary.
            let secondary_bg: Retained<UIColor> =
                unsafe { msg_send![UIColor::class(), secondarySystemBackgroundColor] };
            let _: () = unsafe { msg_send![&*view2, setBackgroundColor: &*secondary_bg] };

            // Set self as view2's delegate so we can react to text changes.
            let _: () = unsafe { msg_send![&*view2, setDelegate: self] };

            // Set a known font on view2 and use it to measure line height.
            // A freshly-init'd UITextView can return nil for `font`, so set it explicitly.
            let font: Retained<UIFont> =
                unsafe { msg_send![UIFont::class(), systemFontOfSize: 17.0_f64] };
            let _: () = unsafe { msg_send![&*view2, setFont: &*font] };

            let line_h: CGFloat = unsafe { msg_send![&*font, lineHeight] };
            let inset: UIEdgeInsets = unsafe { msg_send![&*view2, textContainerInset] };
            let one_line = line_h + inset.top + inset.bottom;
            let three_line = line_h * 3.0 + inset.top + inset.bottom;

            self.ivars().one_line_height.set(one_line);
            self.ivars().three_line_height.set(three_line);
            self.ivars().view2_height.set(one_line);

            // Add as subviews.
            if let Some(view) = self.view() {
                if let Some(ref v1) = view1_opt {
                    let _: () = unsafe { msg_send![&*view, addSubview: &**v1] };
                }
                let _: () = unsafe { msg_send![&*view, addSubview: &*view2] };
            }

            // ---------- Coordinator + tap recognizer for view1 ----------
            let view1_obj: *const AnyObject = view1_opt
                .as_ref()
                .map(|v| &**v as *const _ as *const AnyObject)
                .unwrap_or(std::ptr::null());
            let view2_obj: *const AnyObject = &*view2 as *const _ as *const AnyObject;
            let coordinator = BtIosKeyboardCoordinator::new(mtm, view1_obj, view2_obj);

            let mut tap_opt: Option<Retained<UITapGestureRecognizer>> = None;
            if let Some(ref v1) = view1_opt {
                // Give view1 a weak ref back to the coordinator (for touchesBegan routing).
                v1.set_coordinator(&*coordinator as *const _ as *const AnyObject);

                // Belt + braces: also install a UITapGestureRecognizer on view1.
                let tap: Retained<UITapGestureRecognizer> = unsafe {
                    let alloc = mtm.alloc::<UITapGestureRecognizer>();
                    msg_send![
                        alloc,
                        initWithTarget: &*coordinator,
                        action: sel!(handleView1Tap:),
                    ]
                };
                let _: () = unsafe { msg_send![&**v1, addGestureRecognizer: &*tap] };
                tap_opt = Some(tap);
            }
            *self.ivars().view1_tap.borrow_mut() = tap_opt;

            // ---------- ModeState (shared) ----------
            let self_ptr = self as *const Self as *const AnyObject;
            let mode_state = Rc::new(ModeState::new(self_ptr));
            coordinator.set_mode_state(&mode_state);
            if let Some(ref v1) = view1_opt {
                v1.set_mode_state(Rc::as_ptr(&mode_state));
            }

            // ---------- Build all bottom-row variants up front ----------
            // State1+State2 share a no-newline keybar; State3 uses one
            // with the newline chip.
            let keybar_std = crate::keybar::make_keybar_with(
                mtm,
                &coordinator,
                crate::keybar::ChipSet::Standard,
            );
            let keybar_nl = crate::keybar::make_keybar_with(
                mtm,
                &coordinator,
                crate::keybar::ChipSet::WithNewline,
            );

            // State1 Run/Send chip — same row as the keybar.
            let send_chip = make_action_chip(
                mtm,
                "return",
                "Run",
                true,
                coordinator.as_ref(),
                sel!(keybarSend:),
            );
            // State2 composer chip — icon only.
            let composer_chip = make_action_chip(
                mtm,
                "square.and.pencil",
                "",
                true,
                coordinator.as_ref(),
                sel!(openComposer:),
            );
            // State3 close-composer chip — icon only.
            let close_chip = make_action_chip(
                mtm,
                "xmark",
                "",
                true,
                coordinator.as_ref(),
                sel!(closeComposer:),
            );

            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addSubview: &*keybar_std] };
                let _: () = unsafe { msg_send![&*view, addSubview: &*keybar_nl] };
                let _: () = unsafe { msg_send![&*view, addSubview: &*send_chip] };
                let _: () = unsafe { msg_send![&*view, addSubview: &*composer_chip] };
                let _: () = unsafe { msg_send![&*view, addSubview: &*close_chip] };
            }

            // ---------- Debug HUD (FPS + HW keyboard) top-right ----------
            let hud = crate::debug_hud::BtIosDebugHUD::new(mtm);
            hud.set_mode_state(&mode_state);
            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addSubview: &*hud] };
            }

            // Store strong refs.
            *self.ivars().view1.borrow_mut() = view1_opt;
            *self.ivars().view2.borrow_mut() = Some(view2);
            *self.ivars().keybar.borrow_mut() = Some(keybar_std);
            *self.ivars().keybar_with_newline.borrow_mut() = Some(keybar_nl);
            *self.ivars().send_chip.borrow_mut() = Some(send_chip);
            *self.ivars().composer_chip.borrow_mut() = Some(composer_chip);
            *self.ivars().close_composer_chip.borrow_mut() = Some(close_chip);
            *self.ivars().coordinator.borrow_mut() = Some(coordinator);
            *self.ivars().debug_hud.borrow_mut() = Some(hud);
            *self.ivars().mode_state.borrow_mut() = Some(mode_state);

            // Initial visibility (State2 by default).
            self.apply_mode_visibility();
        }

        #[unsafe(method(backButtonTapped))]
        fn back_button_tapped(&self) {
            let ivars = self.ivars();
            if let Some(cb) = ivars.on_back.get() {
                let ctx = ivars.ctx.get();
                unsafe { cb(ctx) };
            }
        }

        #[unsafe(method(viewDidLayoutSubviews))]
        fn view_did_layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };

            let view = match self.view() {
                Some(v) => v,
                None => return,
            };

            let bounds: CGRect = unsafe { msg_send![&*view, bounds] };
            let insets: UIEdgeInsets = unsafe { msg_send![&*view, safeAreaInsets] };

            let v2_h = self.ivars().view2_height.get();
            let width = bounds.size.width;
            let safe_top = insets.top;
            let safe_bottom = insets.bottom;
            let keybar_h: CGFloat = crate::keybar::BAR_HEIGHT;

            let mode = self
                .ivars()
                .mode_state
                .borrow()
                .as_ref()
                .map(|m| m.mode())
                .unwrap_or(InputMode::State2);

            // ---- Bottom strip ---------------------------------------------
            // One composer surface (view2) is reused as the input row in
            // both State1 and State3 — same auto-growing UITextView, just
            // a different trailing action chip + keybar variant. Layout,
            // top → bottom:
            //
            //   State1: view1 | view2 | [tab esc ctrl]                 [Run]
            //   State2: view1         | [tab esc ctrl]            [Composer]
            //   State3: view1 | view2 | [newline tab esc ctrl]       [Close]
            let bottom_y_end = bounds.size.height - safe_bottom;
            let keybar_y = bottom_y_end - keybar_h;
            let chip_inset: CGFloat = 6.0;

            // Active action chip — sized to its intrinsic content so the
            // icon-only Composer/Close are narrower than the text-bearing
            // Run chip.
            let active_chip: Option<Retained<UIButton>> = match mode {
                InputMode::State1 => self.ivars().send_chip.borrow().clone(),
                InputMode::State2 => self.ivars().composer_chip.borrow().clone(),
                InputMode::State3 => self.ivars().close_composer_chip.borrow().clone(),
            };
            let chip_w: CGFloat = if let Some(ref c) = active_chip {
                let sz: CGSize = unsafe { msg_send![&**c, intrinsicContentSize] };
                sz.width.max(36.0)
            } else {
                0.0
            };
            let chip_h: CGFloat = keybar_h - 8.0;
            let chip_x = width - chip_w - chip_inset;
            let chip_y = keybar_y + 4.0;
            let chip_frame = CGRect {
                origin: CGPoint { x: chip_x, y: chip_y },
                size: CGSize { width: chip_w, height: chip_h },
            };

            // Keybar (the [tab esc ctrl] strip) shares the row with the chip.
            let keybar_w = (chip_x - chip_inset).max(0.0);
            let keybar_frame = CGRect {
                origin: CGPoint { x: 0.0, y: keybar_y },
                size: CGSize { width: keybar_w, height: keybar_h },
            };

            // view2 (composer) sits above the keybar row in State1 + State3.
            let v2_frame = CGRect {
                origin: CGPoint { x: 0.0, y: keybar_y - v2_h },
                size: CGSize { width, height: v2_h },
            };

            // view1 fills everything above view2 (or the keybar in State2).
            let v1_bottom = match mode {
                InputMode::State1 | InputMode::State3 => keybar_y - v2_h,
                InputMode::State2 => keybar_y,
            };
            let v1_height = (v1_bottom - safe_top).max(0.0);
            let v1_frame = CGRect {
                origin: CGPoint { x: 0.0, y: safe_top },
                size: CGSize { width, height: v1_height },
            };

            if let Some(ref v1) = *self.ivars().view1.borrow() {
                let _: () = unsafe { msg_send![&**v1, setFrame: v1_frame] };
            }
            if let Some(ref v2) = *self.ivars().view2.borrow() {
                let _: () = unsafe { msg_send![&**v2, setFrame: v2_frame] };
            }

            // Keybar row leading widget (the [tab esc ctrl] strip).
            // In all three states *some* keybar variant occupies that strip.
            let active_keybar = match mode {
                InputMode::State1 | InputMode::State2 => self.ivars().keybar.borrow().clone(),
                InputMode::State3 => self.ivars().keybar_with_newline.borrow().clone(),
            };
            if let Some(ref kb) = active_keybar {
                let _: () = unsafe { msg_send![&***kb, setFrame: keybar_frame] };
            }

            // Frame the active trailing chip (the inactive ones are hidden
            // by apply_mode_visibility but we still keep their frames sane).
            if let Some(ref c) = *self.ivars().send_chip.borrow() {
                let _: () = unsafe { msg_send![&**c, setFrame: chip_frame] };
            }
            if let Some(ref c) = *self.ivars().composer_chip.borrow() {
                let _: () = unsafe { msg_send![&**c, setFrame: chip_frame] };
            }
            if let Some(ref c) = *self.ivars().close_composer_chip.borrow() {
                let _: () = unsafe { msg_send![&**c, setFrame: chip_frame] };
            }

            // Debug HUD — pinned to top-right inside the safe area.
            let hud_borrow = self.ivars().debug_hud.borrow();
            if let Some(ref hud) = *hud_borrow {
                let hud_w = crate::debug_hud::HUD_WIDTH;
                let hud_h = crate::debug_hud::HUD_HEIGHT;
                let hud_frame = CGRect {
                    origin: CGPoint {
                        x: width - insets.right - hud_w - 8.0,
                        y: safe_top + 8.0,
                    },
                    size: CGSize { width: hud_w, height: hud_h },
                };
                let _: () = unsafe { msg_send![&**hud, setFrame: hud_frame] };
            }
        }

        #[unsafe(method(textViewDidChange:))]
        fn text_view_did_change(&self, text_view: &UITextView) {
            // Tell the coordinator about the new text so it can pre-compute
            // view1's next background color.
            let text: Retained<NSString> = unsafe { msg_send![text_view, text] };
            let coord_borrow = self.ivars().coordinator.borrow();
            if let Some(ref coord) = *coord_borrow {
                let _: () = unsafe { msg_send![&**coord, notifyText2Changed: &*text] };
            }

            // Re-clamp view2's height.
            let content_height: CGFloat = {
                let s: CGSize = unsafe { msg_send![text_view, contentSize] };
                s.height
            };
            let min_h = self.ivars().one_line_height.get();
            let max_h = self.ivars().three_line_height.get();
            let clamped = content_height.max(min_h).min(max_h);
            let prev = self.ivars().view2_height.get();
            if (clamped - prev).abs() > 0.5 {
                self.ivars().view2_height.set(clamped);
                self.relayout();
            }
        }

        #[unsafe(method(textViewDidBeginEditing:))]
        fn text_view_did_begin_editing(&self, _text_view: &UITextView) {
            // view2 became first responder via its own tap — tell the
            // coordinator so currentFocus stays in sync.
            let coord_borrow = self.ivars().coordinator.borrow();
            if let Some(ref coord) = *coord_borrow {
                let _: () = unsafe { msg_send![&**coord, focusView2] };
            }
        }

        /// Posted by `ModeState::notify`. Re-applies subview visibility
        /// and triggers a relayout.
        #[unsafe(method(modeStateDidChange))]
        fn mode_state_did_change(&self) {
            self.apply_mode_visibility();
            self.relayout();
            // Resign first responder on every mode flip so UIKit doesn't
            // keep a now-disallowed view as the active responder.
            let mode = self
                .ivars()
                .mode_state
                .borrow()
                .as_ref()
                .map(|m| m.mode())
                .unwrap_or(InputMode::State2);
            match mode {
                InputMode::State1 | InputMode::State3 => {
                    // view2 owns input. Drop view1's responder if it has
                    // it (canBecomeFirstResponder is now false here).
                    if let Some(ref v1) = *self.ivars().view1.borrow() {
                        let _: bool = unsafe { msg_send![&**v1, resignFirstResponder] };
                    }
                    let coord_borrow = self.ivars().coordinator.borrow();
                    if let Some(ref coord) = *coord_borrow {
                        let _: () = unsafe { msg_send![&**coord, focusView2] };
                    }
                }
                InputMode::State2 => {
                    if let Some(ref v2) = *self.ivars().view2.borrow() {
                        let _: bool = unsafe { msg_send![&**v2, resignFirstResponder] };
                    }
                }
            }
        }
    }
);

impl RsTerminalViewController {
    fn relayout(&self) {
        if let Some(view) = self.view() {
            let _: () = unsafe { msg_send![&*view, setNeedsLayout] };
            let _: () = unsafe { msg_send![&*view, layoutIfNeeded] };
        }
    }

    /// Per-state subview visibility. Show / hide instead of remove / add so
    /// strong refs and gesture recognizers stay attached.
    fn apply_mode_visibility(&self) {
        let mode = self
            .ivars()
            .mode_state
            .borrow()
            .as_ref()
            .map(|m| m.mode())
            .unwrap_or(InputMode::State2);

        // view2 (the shared composer) — visible in State1 and State3,
        // hidden in State2.
        let v2_hidden = matches!(mode, InputMode::State2);
        if let Some(ref v2) = *self.ivars().view2.borrow() {
            let _: () = unsafe { msg_send![&**v2, setHidden: v2_hidden] };
        }
        // Standard keybar — State1 + State2 share the [tab esc ctrl]
        // strip; State3 swaps in the newline variant.
        let std_hidden = matches!(mode, InputMode::State3);
        if let Some(ref kb) = *self.ivars().keybar.borrow() {
            let _: () = unsafe { msg_send![&**kb, setHidden: std_hidden] };
        }
        // Newline keybar — State3 only.
        let nl_hidden = !matches!(mode, InputMode::State3);
        if let Some(ref kb) = *self.ivars().keybar_with_newline.borrow() {
            let _: () = unsafe { msg_send![&**kb, setHidden: nl_hidden] };
        }
        // Send chip — State1 only.
        let send_hidden = !matches!(mode, InputMode::State1);
        if let Some(ref c) = *self.ivars().send_chip.borrow() {
            let _: () = unsafe { msg_send![&**c, setHidden: send_hidden] };
        }
        // Composer chip — State2 only.
        let comp_hidden = !matches!(mode, InputMode::State2);
        if let Some(ref c) = *self.ivars().composer_chip.borrow() {
            let _: () = unsafe { msg_send![&**c, setHidden: comp_hidden] };
        }
        // Close-composer chip — State3 only.
        let close_hidden = !matches!(mode, InputMode::State3);
        if let Some(ref c) = *self.ivars().close_composer_chip.borrow() {
            let _: () = unsafe { msg_send![&**c, setHidden: close_hidden] };
        }

        // Disable view1's tap recognizer in State1 / State3 — view1 cannot
        // become first responder, so tapping it shouldn't try to.
        let tap_enabled = matches!(mode, InputMode::State2);
        if let Some(ref tap) = *self.ivars().view1_tap.borrow() {
            let _: () = unsafe { msg_send![&**tap, setEnabled: tap_enabled] };
        }
    }
}

pub(crate) unsafe fn create_vc(
    on_back: Option<BtIosBackCallback>,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };

    let vc: Retained<RsTerminalViewController> =
        unsafe { msg_send![mtm.alloc::<RsTerminalViewController>(), init] };

    vc.ivars().on_back.set(on_back);
    vc.ivars().ctx.set(ctx);

    Retained::into_raw(vc) as *mut std::ffi::c_void
}

pub(crate) unsafe fn release_vc(vc_ptr: *mut std::ffi::c_void) {
    if vc_ptr.is_null() {
        return;
    }
    let _ = unsafe { Retained::from_raw(vc_ptr as *mut RsTerminalViewController) };
}
