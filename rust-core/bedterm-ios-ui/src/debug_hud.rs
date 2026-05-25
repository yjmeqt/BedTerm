//! DEBUG-only HUD overlay — FPS + hardware-keyboard indicator pinned to the
//! top-right corner of the Rust VC.
//!
//! Mirrors the Swift `TerminalScreen.geomHUD`: monospace 10 pt text in a
//! muted-gray pill (15 %-opacity background, 4 pt corner radius). FPS is
//! sampled by a `CADisplayLink` over a 1 s sliding window; hardware-keyboard
//! state is re-polled on the same cadence via `GCKeyboard.coalescedKeyboard`.

use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{declare_class, msg_send, msg_send_id, sel, ClassType, DeclaredClass};
use objc2_foundation::{
    MainThreadMarker, NSNotificationCenter, NSRunLoop, NSRunLoopCommonModes, NSString,
};
use objc2_quartz_core::CADisplayLink;
use objc2_ui_kit::{UIColor, UIFont, UIFontWeight, UILabel, UIView};
use std::cell::{Cell, RefCell};

pub(crate) const HUD_WIDTH: CGFloat = 70.0;
const ROW_HEIGHT: CGFloat = 16.0;
const ROW_SPACING: CGFloat = 2.0;
pub(crate) const HUD_HEIGHT: CGFloat = ROW_HEIGHT * 2.0 + ROW_SPACING;

#[derive(Default)]
pub struct Ivars {
    fps_label: RefCell<Option<Retained<UILabel>>>,
    kbd_row: RefCell<Option<Retained<UIView>>>,
    kbd_label: RefCell<Option<Retained<UILabel>>>,
    display_link: RefCell<Option<Retained<CADisplayLink>>>,
    frame_count: Cell<u32>,
    window_start: Cell<f64>,
    last_attached: Cell<bool>,
    /// Software keyboard currently visible (tracked via `UIKeyboardWillShow`
    /// / `UIKeyboardWillHide`). When the SW keyboard is up, HW kbd is not in
    /// effective use so we hide the indicator.
    sw_keyboard_visible: Cell<bool>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

declare_class!(
    /// Top-right floating debug HUD. Subclasses `UIView`.
    pub struct BtIosDebugHUD;

    unsafe impl ClassType for BtIosDebugHUD {
        type Super = UIView;
        type Mutability = objc2::mutability::MainThreadOnly;
        const NAME: &'static str = "BtIosDebugHUD";
    }

    impl DeclaredClass for BtIosDebugHUD {
        type Ivars = Ivars;
    }

    unsafe impl BtIosDebugHUD {
        #[method_id(initWithFrame:)]
        fn init_with_frame(this: Allocated<Self>, frame: CGRect) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            let this: Retained<Self> = unsafe { msg_send_id![super(this), initWithFrame: frame] };
            Some(this)
        }

        /// `UIKeyboardWillShowNotification` observer.
        #[method(onKeyboardWillShow:)]
        fn on_keyboard_will_show(&self, _note: &AnyObject) {
            self.ivars().sw_keyboard_visible.set(true);
            self.refresh_keyboard();
        }

        /// `UIKeyboardWillHideNotification` observer.
        #[method(onKeyboardWillHide:)]
        fn on_keyboard_will_hide(&self, _note: &AnyObject) {
            self.ivars().sw_keyboard_visible.set(false);
            self.refresh_keyboard();
        }

        /// `CADisplayLink` target — fires every screen refresh.
        #[method(onDisplayTick:)]
        fn on_display_tick(&self, link: &CADisplayLink) {
            let ts: f64 = unsafe { link.timestamp() };
            if self.ivars().window_start.get() == 0.0 {
                self.ivars().window_start.set(ts);
            }
            self.ivars().frame_count.set(self.ivars().frame_count.get() + 1);
            let elapsed = ts - self.ivars().window_start.get();
            if elapsed >= 1.0 {
                let fps =
                    (f64::from(self.ivars().frame_count.get()) / elapsed).round() as i32;
                self.refresh_fps(fps);
                self.refresh_keyboard();
                self.ivars().frame_count.set(0);
                self.ivars().window_start.set(ts);
            }
        }
    }
);

impl BtIosDebugHUD {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let frame = CGRect {
            origin: CGPoint::default(),
            size: CGSize {
                width: HUD_WIDTH,
                height: HUD_HEIGHT,
            },
        };
        let this: Retained<Self> =
            unsafe { msg_send_id![mtm.alloc::<Self>(), initWithFrame: frame] };

        // Two chip rows stacked vertically. We position them manually rather
        // than via a UIStackView to keep the dependency surface small.
        let (fps_row, fps_label) = make_row(mtm, 0.0);
        let (kbd_row, kbd_label) = make_row(mtm, ROW_HEIGHT + ROW_SPACING);
        unsafe {
            this.addSubview(&fps_row);
            this.addSubview(&kbd_row);
        }
        // Hide the keyboard row until something is attached.
        let _: () = unsafe { msg_send![&*kbd_row, setHidden: true] };

        *this.ivars().fps_label.borrow_mut() = Some(fps_label);
        *this.ivars().kbd_row.borrow_mut() = Some(kbd_row);
        *this.ivars().kbd_label.borrow_mut() = Some(kbd_label);

        // Start the display link — retains `this` for the runloop's lifetime.
        let link: Retained<CADisplayLink> =
            unsafe { CADisplayLink::displayLinkWithTarget_selector(&*this, sel!(onDisplayTick:)) };
        unsafe {
            let runloop = NSRunLoop::mainRunLoop();
            link.addToRunLoop_forMode(&runloop, NSRunLoopCommonModes);
        }
        *this.ivars().display_link.borrow_mut() = Some(link);

        // Subscribe to software-keyboard show/hide notifications so the HW
        // kbd badge reflects effective use, not just connection state.
        unsafe {
            let center = NSNotificationCenter::defaultCenter();
            let show = NSString::from_str("UIKeyboardWillShowNotification");
            let hide = NSString::from_str("UIKeyboardWillHideNotification");
            let _: () = msg_send![&*center,
                addObserver: &*this,
                selector: sel!(onKeyboardWillShow:),
                name: &*show,
                object: std::ptr::null::<AnyObject>(),
            ];
            let _: () = msg_send![&*center,
                addObserver: &*this,
                selector: sel!(onKeyboardWillHide:),
                name: &*hide,
                object: std::ptr::null::<AnyObject>(),
            ];
        }

        this
    }

    /// Tear down the display link so the HUD can be released.
    #[allow(dead_code)]
    pub(crate) fn invalidate(&self) {
        if let Some(link) = self.ivars().display_link.borrow_mut().take() {
            unsafe { link.invalidate() };
        }
    }

    fn refresh_fps(&self, fps: i32) {
        let label_ref = self.ivars().fps_label.borrow();
        let Some(label) = label_ref.as_ref() else {
            return;
        };
        let s = NSString::from_str(&format!("{fps} fps"));
        let _: () = unsafe { msg_send![&**label, setText: &*s] };
    }

    fn refresh_keyboard(&self) {
        // "HW kbd in effective use" = hardware is connected AND the software
        // keyboard is not currently up.
        let effective = hardware_keyboard_attached() && !self.ivars().sw_keyboard_visible.get();
        if effective == self.ivars().last_attached.get() && self.frame_count_initialized() {
            return;
        }
        self.ivars().last_attached.set(effective);
        let row_ref = self.ivars().kbd_row.borrow();
        let Some(row) = row_ref.as_ref() else { return };
        let _: () = unsafe { msg_send![&**row, setHidden: !effective] };
        if effective {
            let label_ref = self.ivars().kbd_label.borrow();
            if let Some(label) = label_ref.as_ref() {
                let s = NSString::from_str("HW kbd");
                let _: () = unsafe { msg_send![&**label, setText: &*s] };
            }
        }
    }

    /// True after the first FPS tick — guards the keyboard-row toggle so it
    /// runs at least once at startup.
    fn frame_count_initialized(&self) -> bool {
        self.ivars().window_start.get() > 0.0
    }
}

/// Build one HUD row at `y_offset`: a rounded-corner container holding a
/// monospace `UILabel`. Returns `(container, label)`.
fn make_row(mtm: MainThreadMarker, y_offset: CGFloat) -> (Retained<UIView>, Retained<UILabel>) {
    let row_frame = CGRect {
        origin: CGPoint {
            x: 0.0,
            y: y_offset,
        },
        size: CGSize {
            width: HUD_WIDTH,
            height: ROW_HEIGHT,
        },
    };
    let row: Retained<UIView> =
        unsafe { msg_send_id![mtm.alloc::<UIView>(), initWithFrame: row_frame] };

    // 15 %-opacity gray pill.
    let bg: Retained<UIColor> = unsafe {
        let base: Retained<UIColor> = msg_send_id![UIColor::class(), secondaryLabelColor];
        msg_send_id![&*base, colorWithAlphaComponent: 0.15_f64]
    };
    let _: () = unsafe { msg_send![&*row, setBackgroundColor: &*bg] };

    let layer: Retained<AnyObject> = unsafe { msg_send_id![&*row, layer] };
    let _: () = unsafe { msg_send![&*layer, setCornerRadius: 4.0_f64] };
    let _: () = unsafe { msg_send![&*layer, setMasksToBounds: true] };

    let label: Retained<UILabel> =
        unsafe { msg_send_id![mtm.alloc::<UILabel>(), initWithFrame: row.bounds()] };
    let _: () = unsafe { msg_send![&*label, setAutoresizingMask: (1u64 << 1) | (1u64 << 4)] };
    let font: Retained<UIFont> =
        unsafe { UIFont::monospacedSystemFontOfSize_weight(10.0, 0.0 as UIFontWeight) };
    unsafe { label.setFont(Some(&font)) };
    let fg: Retained<UIColor> = unsafe { msg_send_id![UIColor::class(), secondaryLabelColor] };
    unsafe { label.setTextColor(Some(&fg)) };
    let _: () = unsafe { msg_send![&*label, setTextAlignment: 1_i64] }; // .center
    let _: () = unsafe { msg_send![&*row, addSubview: &*label] };

    (row, label)
}

/// Polls `GCKeyboard.coalescedKeyboard` via runtime class lookup so we don't
/// have to depend on `objc2-game-controller`.
fn hardware_keyboard_attached() -> bool {
    let Some(cls) = AnyClass::get("GCKeyboard") else {
        return false;
    };
    let kb: *const AnyObject = unsafe { msg_send![cls, coalescedKeyboard] };
    !kb.is_null()
}
