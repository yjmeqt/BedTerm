//! The experimental Rust-backed view controller.

use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize, UIEdgeInsets};
use crate::BtRsBackCallback;
use objc2::rc::{Allocated, Retained};
use objc2::{declare_class, msg_send, msg_send_id, sel, ClassType, DeclaredClass};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    UIBarButtonItem, UIBarButtonItemStyle, UIColor, UIFont, UINavigationItem, UITextView,
    UIViewController,
};
use std::cell::{Cell, RefCell};

/// Per-instance state stored as ivars.
#[derive(Default)]
pub struct Ivars {
    /// C callback to fire on back-tap (may be None).
    on_back: Cell<Option<BtRsBackCallback>>,
    /// Context pointer for the callback. Only accessed on the main thread.
    ctx: Cell<*mut std::ffi::c_void>,
    /// Upper text view — fills the area between safe-area top and view2.top.
    view1: RefCell<Option<Retained<UITextView>>>,
    /// Lower composer text view — sits above the keyboard, 1–3 lines tall.
    view2: RefCell<Option<Retained<UITextView>>>,
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

declare_class!(
    /// Experimental Rust-backed terminal view controller.
    pub struct RsTerminalViewController;

    unsafe impl ClassType for RsTerminalViewController {
        type Super = UIViewController;
        type Mutability = objc2::mutability::MainThreadOnly;
        const NAME: &'static str = "BtRsTerminalViewController";
    }

    impl DeclaredClass for RsTerminalViewController {
        type Ivars = Ivars;
    }

    unsafe impl RsTerminalViewController {
        #[method_id(init)]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send_id![super(this), init] }
        }

        #[method(viewDidLoad)]
        fn view_did_load(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLoad] };

            let mtm = unsafe { MainThreadMarker::new_unchecked() };

            // Background colour — system background so it respects dark mode.
            let bg: Retained<UIColor> =
                unsafe { msg_send_id![UIColor::class(), systemBackgroundColor] };
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
                unsafe { msg_send_id![self, navigationItem] };
            unsafe { nav_item.setLeftBarButtonItem(Some(&back_btn)) };

            // ---------- Text views ----------
            let zero = CGRect::default();

            // view1 — main editable area
            let view1: Retained<UITextView> =
                unsafe { msg_send_id![mtm.alloc::<UITextView>(), initWithFrame: zero] };
            // view2 — composer that hugs the keyboard
            let view2: Retained<UITextView> =
                unsafe { msg_send_id![mtm.alloc::<UITextView>(), initWithFrame: zero] };

            // Make view2 visually distinct so the user can see the boundary.
            let secondary_bg: Retained<UIColor> =
                unsafe { msg_send_id![UIColor::class(), secondarySystemBackgroundColor] };
            let _: () = unsafe { msg_send![&*view2, setBackgroundColor: &*secondary_bg] };

            // Set self as view2's delegate so we can react to text changes.
            let _: () = unsafe { msg_send![&*view2, setDelegate: self] };

            // Set a known font on both views and use it to measure line height.
            // A freshly-init'd UITextView can return nil for `font`, so set it explicitly.
            let font: Retained<UIFont> =
                unsafe { msg_send_id![UIFont::class(), systemFontOfSize: 17.0_f64] };
            let _: () = unsafe { msg_send![&*view1, setFont: &*font] };
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
                let _: () = unsafe { msg_send![&*view, addSubview: &*view1] };
                let _: () = unsafe { msg_send![&*view, addSubview: &*view2] };
            }

            // Store strong refs.
            *self.ivars().view1.borrow_mut() = Some(view1);
            *self.ivars().view2.borrow_mut() = Some(view2);
        }

        #[method(backButtonTapped)]
        fn back_button_tapped(&self) {
            let ivars = self.ivars();
            if let Some(cb) = ivars.on_back.get() {
                let ctx = ivars.ctx.get();
                unsafe { cb(ctx) };
            }
        }

        #[method(viewDidLayoutSubviews)]
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

            // SwiftUI shrinks the host view when the keyboard appears, so
            // bounds.size.height already reflects the keyboard-aware area.
            let v2_y = bounds.size.height - safe_bottom - v2_h;

            let v2_frame = CGRect {
                origin: CGPoint { x: 0.0, y: v2_y },
                size: CGSize { width, height: v2_h },
            };
            let v1_height = (v2_y - safe_top).max(0.0);
            let v1_frame = CGRect {
                origin: CGPoint { x: 0.0, y: safe_top },
                size: CGSize { width, height: v1_height },
            };

            let v1_borrow = self.ivars().view1.borrow();
            if let Some(ref v1) = *v1_borrow {
                let _: () = unsafe { msg_send![&**v1, setFrame: v1_frame] };
            }
            let v2_borrow = self.ivars().view2.borrow();
            if let Some(ref v2) = *v2_borrow {
                let _: () = unsafe { msg_send![&**v2, setFrame: v2_frame] };
            }
        }

        #[method(textViewDidChange:)]
        fn text_view_did_change(&self, _text_view: &UITextView) {
            let content_height: CGFloat = {
                let v2_borrow = self.ivars().view2.borrow();
                match v2_borrow.as_ref() {
                    Some(v2) => {
                        let s: CGSize = unsafe { msg_send![&**v2, contentSize] };
                        s.height
                    }
                    None => return,
                }
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
    }
);

impl RsTerminalViewController {
    fn relayout(&self) {
        if let Some(view) = self.view() {
            let _: () = unsafe { msg_send![&*view, setNeedsLayout] };
            let _: () = unsafe { msg_send![&*view, layoutIfNeeded] };
        }
    }
}

pub(crate) unsafe fn create_vc(
    on_back: Option<BtRsBackCallback>,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };

    let vc: Retained<RsTerminalViewController> =
        unsafe { msg_send_id![mtm.alloc::<RsTerminalViewController>(), init] };

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
