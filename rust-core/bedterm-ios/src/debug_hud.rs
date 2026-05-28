//! DEBUG-only HUD overlay — FPS + hardware-keyboard indicator pinned to the
//! top-right corner of the Rust VC.
//!
//! Mirrors the Swift `TerminalScreen.geomHUD`: monospace 10 pt text in a
//! muted-gray pill (15 %-opacity background, 4 pt corner radius). FPS is
//! sampled by a `CADisplayLink` over a 1 s sliding window; hardware-keyboard
//! state is re-polled on the same cadence via `GCKeyboard.coalescedKeyboard`.

use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use crate::input_mode::{InputMode, ModeState};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{
    define_class, msg_send, sel, AnyThread, ClassType, DefinedClass, MainThreadMarker,
    MainThreadOnly,
};
use objc2_foundation::{NSNotificationCenter, NSRunLoop, NSRunLoopCommonModes, NSString};
use objc2_quartz_core::CADisplayLink;
use objc2_ui_kit::{UIButton, UIColor, UIFont, UIFontWeight, UILabel, UIView};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(crate) const HUD_WIDTH: CGFloat = 70.0;
const ROW_HEIGHT: CGFloat = 16.0;
const ROW_SPACING: CGFloat = 2.0;
// 5 rows: fps / hw_kbd / geom (cols×rows) / mode / input-switcher.
pub(crate) const HUD_HEIGHT: CGFloat = ROW_HEIGHT * 5.0 + ROW_SPACING * 4.0;

/// `UIControlEventTouchUpInside` = 1 << 6.
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;

#[derive(Default)]
pub struct Ivars {
    fps_label: RefCell<Option<Retained<UILabel>>>,
    kbd_row: RefCell<Option<Retained<UIView>>>,
    kbd_label: RefCell<Option<Retained<UILabel>>>,
    /// Geometry chip label — "{cols}×{rows}". Mirror of the Swift
    /// `geomHUD`'s cols×rows chip. Note: the Swift HUD also showed
    /// `[alt,bp]` terminal-mode flags from `session.mode`; the Rust VC
    /// doesn't track session terminal modes yet, so we only display the
    /// PTY grid here. TODO: surface alt-screen / bracketed-paste once
    /// the Rust session wrapper exposes them.
    geom_label: RefCell<Option<Retained<UILabel>>>,
    /// Mode chip label — "mode={input|idle|composer}".
    mode_label: RefCell<Option<Retained<UILabel>>>,
    display_link: RefCell<Option<Retained<CADisplayLink>>>,
    frame_count: Cell<u32>,
    window_start: Cell<f64>,
    last_attached: Cell<bool>,
    /// Software keyboard currently visible (tracked via `UIKeyboardWillShow`
    /// / `UIKeyboardWillHide`). When the SW keyboard is up, HW kbd is not in
    /// effective use so we hide the indicator.
    sw_keyboard_visible: Cell<bool>,
    /// Shared mode state — used by the input-mode switcher row to drive
    /// State1 vs State2/3 transitions and to highlight the active button.
    mode_state: RefCell<Option<Rc<ModeState>>>,
    /// Input-mode switcher buttons (1, 2/3).
    input1_btn: RefCell<Option<Retained<UIButton>>>,
    input23_btn: RefCell<Option<Retained<UIButton>>>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// Top-right floating debug HUD. Subclasses `UIView`.
    #[unsafe(super(UIView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosDebugHUD"]
    #[ivars = Ivars]
    pub struct BtIosDebugHUD;

    impl BtIosDebugHUD {
        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(this: Allocated<Self>, frame: CGRect) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
            Some(this)
        }

        /// `UIKeyboardWillShowNotification` observer.
        #[unsafe(method(onKeyboardWillShow:))]
        fn on_keyboard_will_show(&self, _note: &AnyObject) {
            self.ivars().sw_keyboard_visible.set(true);
            self.refresh_keyboard();
        }

        /// `UIKeyboardWillHideNotification` observer.
        #[unsafe(method(onKeyboardWillHide:))]
        fn on_keyboard_will_hide(&self, _note: &AnyObject) {
            self.ivars().sw_keyboard_visible.set(false);
            self.refresh_keyboard();
        }

        /// HUD input-mode switcher — force State1.
        #[unsafe(method(onSelectInput1:))]
        fn on_select_input1(&self, _sender: &AnyObject) {
            let ms_ref = self.ivars().mode_state.borrow();
            let Some(ms) = ms_ref.as_ref().cloned() else { return };
            drop(ms_ref);
            ms.set_mode(InputMode::State1);
            self.refresh_input_buttons();
        }

        /// HUD input-mode switcher — leave State1 (defaults to State2).
        #[unsafe(method(onSelectInput23:))]
        fn on_select_input23(&self, _sender: &AnyObject) {
            let ms_ref = self.ivars().mode_state.borrow();
            let Some(ms) = ms_ref.as_ref().cloned() else { return };
            drop(ms_ref);
            if ms.mode() == InputMode::State1 {
                ms.set_mode(InputMode::State2);
            }
            self.refresh_input_buttons();
        }

        /// `CADisplayLink` target — fires every screen refresh.
        #[unsafe(method(onDisplayTick:))]
        fn on_display_tick(&self, link: &CADisplayLink) {
            let ts: f64 = link.timestamp();
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
        let this: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] };

        // Five chip rows stacked vertically. We position them manually
        // rather than via a UIStackView to keep the dependency surface small.
        let row_step = ROW_HEIGHT + ROW_SPACING;
        let (fps_row, fps_label) = make_row(mtm, 0.0);
        let (kbd_row, kbd_label) = make_row(mtm, row_step);
        let (geom_row, geom_label) = make_row(mtm, row_step * 2.0);
        let (mode_row, mode_label) = make_row(mtm, row_step * 3.0);
        let input_y = row_step * 4.0;
        let (input_row, input1_btn, input23_btn) = make_input_switcher_row(mtm, input_y, &this);
        this.addSubview(&fps_row);
        this.addSubview(&kbd_row);
        this.addSubview(&geom_row);
        this.addSubview(&mode_row);
        this.addSubview(&input_row);
        // Hide the keyboard row until something is attached.
        let _: () = unsafe { msg_send![&*kbd_row, setHidden: true] };
        // Seed the new chips with placeholders so they're not blank pills.
        let placeholder_geom = NSString::from_str("?×?");
        let _: () = unsafe { msg_send![&*geom_label, setText: &*placeholder_geom] };
        let placeholder_mode = NSString::from_str("mode=?");
        let _: () = unsafe { msg_send![&*mode_label, setText: &*placeholder_mode] };

        *this.ivars().fps_label.borrow_mut() = Some(fps_label);
        *this.ivars().kbd_row.borrow_mut() = Some(kbd_row);
        *this.ivars().kbd_label.borrow_mut() = Some(kbd_label);
        *this.ivars().geom_label.borrow_mut() = Some(geom_label);
        *this.ivars().mode_label.borrow_mut() = Some(mode_label);
        *this.ivars().input1_btn.borrow_mut() = Some(input1_btn);
        *this.ivars().input23_btn.borrow_mut() = Some(input23_btn);

        // Start the display link — retains `this` for the runloop's lifetime.
        let link: Retained<CADisplayLink> =
            unsafe { CADisplayLink::displayLinkWithTarget_selector(&this, sel!(onDisplayTick:)) };
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
            link.invalidate();
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

    /// Update the cols×rows chip. Idempotent (UIKit will redraw on every
    /// `setText:` regardless; callers should batch but it's cheap).
    pub(crate) fn refresh_geom(&self, cols: u16, rows: u16) {
        let label_ref = self.ivars().geom_label.borrow();
        let Some(label) = label_ref.as_ref() else {
            return;
        };
        let s = NSString::from_str(&format!("{cols}×{rows}"));
        let _: () = unsafe { msg_send![&**label, setText: &*s] };
    }

    /// Update the mode chip. `mode_str` should be the short token
    /// (`input`, `idle`, `composer`) — formatted as `mode={mode_str}`.
    pub(crate) fn refresh_mode(&self, mode_str: &str) {
        let label_ref = self.ivars().mode_label.borrow();
        let Some(label) = label_ref.as_ref() else {
            return;
        };
        let s = NSString::from_str(&format!("mode={mode_str}"));
        let _: () = unsafe { msg_send![&**label, setText: &*s] };
    }

    /// Install the shared mode state and refresh the switcher buttons.
    pub(crate) fn set_mode_state(&self, ms: &Rc<ModeState>) {
        *self.ivars().mode_state.borrow_mut() = Some(ms.clone());
        self.refresh_input_buttons();
    }

    fn refresh_input_buttons(&self) {
        let ms_ref = self.ivars().mode_state.borrow();
        let Some(ms) = ms_ref.as_ref() else { return };
        let mode = ms.mode();
        drop(ms_ref);
        let active_color: Retained<UIColor> =
            unsafe { msg_send![UIColor::class(), systemBlueColor] };
        let inactive_color: Retained<UIColor> =
            unsafe { msg_send![UIColor::class(), secondaryLabelColor] };

        let one_active = matches!(mode, InputMode::State1);
        if let Some(b) = self.ivars().input1_btn.borrow().as_ref() {
            tint_button(
                b,
                if one_active {
                    &active_color
                } else {
                    &inactive_color
                },
            );
        }
        if let Some(b) = self.ivars().input23_btn.borrow().as_ref() {
            tint_button(
                b,
                if !one_active {
                    &active_color
                } else {
                    &inactive_color
                },
            );
        }
    }
}

fn tint_button(button: &UIButton, color: &UIColor) {
    let cfg = button.configuration();
    let Some(cfg) = cfg else { return };
    cfg.setBaseForegroundColor(Some(color));
    button.setConfiguration(Some(&cfg));
}

/// Build the third row: two compact title-only buttons (`1` and `2/3`).
fn make_input_switcher_row(
    mtm: MainThreadMarker,
    y_offset: CGFloat,
    target: &BtIosDebugHUD,
) -> (Retained<UIView>, Retained<UIButton>, Retained<UIButton>) {
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
    let row: Retained<UIView> = unsafe { msg_send![UIView::alloc(mtm), initWithFrame: row_frame] };
    // Fully opaque background so rendered terminal text behind the HUD
    // doesn't bleed through the chip. `systemBackgroundColor` adapts to
    // light/dark appearance and matches the surrounding terminal canvas.
    let bg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), systemBackgroundColor] };
    let _: () = unsafe { msg_send![&*row, setBackgroundColor: &*bg] };
    let _: () = unsafe { msg_send![&*row, setOpaque: true] };
    let layer: Retained<AnyObject> = unsafe { msg_send![&*row, layer] };
    let _: () = unsafe { msg_send![&*layer, setCornerRadius: 4.0_f64] };
    let _: () = unsafe { msg_send![&*layer, setMasksToBounds: true] };

    let btn_w = HUD_WIDTH / 2.0;
    let b1 = make_switcher_button(
        mtm,
        "1",
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: btn_w,
                height: ROW_HEIGHT,
            },
        },
        target as &AnyObject,
        sel!(onSelectInput1:),
    );
    let b23 = make_switcher_button(
        mtm,
        "2/3",
        CGRect {
            origin: CGPoint { x: btn_w, y: 0.0 },
            size: CGSize {
                width: btn_w,
                height: ROW_HEIGHT,
            },
        },
        target as &AnyObject,
        sel!(onSelectInput23:),
    );
    let _: () = unsafe { msg_send![&*row, addSubview: &*b1] };
    let _: () = unsafe { msg_send![&*row, addSubview: &*b23] };

    // Accessibility identifiers for UI tests — see
    // `BedTermUITests/RustTerminalSmokeUITests.swift`.
    crate::a11y::set_a11y_id(&*b1 as &AnyObject, "terminal.rust.hud.input1");
    crate::a11y::set_a11y_id(&*b23 as &AnyObject, "terminal.rust.hud.input23");

    (row, b1, b23)
}

fn make_switcher_button(
    mtm: MainThreadMarker,
    title: &str,
    frame: CGRect,
    target: &AnyObject,
    action: objc2::runtime::Sel,
) -> Retained<UIButton> {
    use objc2_foundation::{NSAttributedString, NSDictionary};
    use objc2_ui_kit::{NSDirectionalEdgeInsets, UIButtonConfiguration};

    let cfg = UIButtonConfiguration::plainButtonConfiguration(mtm);

    let title_ns = NSString::from_str(title);
    let font: Retained<UIFont> =
        UIFont::monospacedSystemFontOfSize_weight(10.0, 0.0 as UIFontWeight);
    let key = NSString::from_str("NSFont");
    let font_obj: Retained<AnyObject> = unsafe { Retained::cast_unchecked(font.clone()) };
    let attrs: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_retained_objects(&[&*key], &[font_obj]);
    let attributed: Retained<NSAttributedString> = unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &title_ns,
            Some(&attrs),
        )
    };
    cfg.setAttributedTitle(Some(&attributed));
    cfg.setContentInsets(NSDirectionalEdgeInsets {
        top: 0.0,
        leading: 2.0,
        bottom: 0.0,
        trailing: 2.0,
    });
    let fg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), secondaryLabelColor] };
    cfg.setBaseForegroundColor(Some(&fg));

    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    button.setConfiguration(Some(&cfg));
    let _: () = unsafe { msg_send![&*button, setFrame: frame] };
    let _: () = unsafe {
        msg_send![&*button,
            addTarget: target,
            action: action,
            forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE,
        ]
    };
    button
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
    let row: Retained<UIView> = unsafe { msg_send![UIView::alloc(mtm), initWithFrame: row_frame] };

    // Fully opaque pill — see `make_input_switcher_row` for rationale:
    // the rendered terminal text sits behind this view, so a translucent
    // background bleeds glyphs through the FPS / HW-kbd chips.
    let bg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), systemBackgroundColor] };
    let _: () = unsafe { msg_send![&*row, setBackgroundColor: &*bg] };
    let _: () = unsafe { msg_send![&*row, setOpaque: true] };

    let layer: Retained<AnyObject> = unsafe { msg_send![&*row, layer] };
    let _: () = unsafe { msg_send![&*layer, setCornerRadius: 4.0_f64] };
    let _: () = unsafe { msg_send![&*layer, setMasksToBounds: true] };

    let label: Retained<UILabel> =
        unsafe { msg_send![UILabel::alloc(mtm), initWithFrame: row.bounds()] };
    let _: () = unsafe { msg_send![&*label, setAutoresizingMask: (1u64 << 1) | (1u64 << 4)] };
    let font: Retained<UIFont> =
        UIFont::monospacedSystemFontOfSize_weight(10.0, 0.0 as UIFontWeight);
    unsafe { label.setFont(Some(&font)) };
    let fg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), secondaryLabelColor] };
    unsafe { label.setTextColor(Some(&fg)) };
    let _: () = unsafe { msg_send![&*label, setTextAlignment: 1_i64] }; // .center
    let _: () = unsafe { msg_send![&*row, addSubview: &*label] };

    (row, label)
}

/// Polls `GCKeyboard.coalescedKeyboard` via runtime class lookup so we don't
/// have to depend on `objc2-game-controller`.
fn hardware_keyboard_attached() -> bool {
    // 0.6's `AnyClass::get` takes `&CStr` (was `&str` in 0.5).
    let Some(cls) = AnyClass::get(c"GCKeyboard") else {
        return false;
    };
    let kb: *const AnyObject = unsafe { msg_send![cls, coalescedKeyboard] };
    !kb.is_null()
}
