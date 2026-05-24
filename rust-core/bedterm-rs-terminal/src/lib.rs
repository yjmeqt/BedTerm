//! A Rust-backed UIViewController exposed via C FFI.
//!
//! This is an experimental crate. The VC is minimal: a plain background,
//! a back button that fires a C callback, and no title (set via SwiftUI
//! `.navigationTitle()` at the call site).

/// Opaque callback type.
pub type BtRsBackCallback = unsafe extern "C" fn(ctx: *mut std::ffi::c_void);

/// Create a `UIViewController *` (returned as `*mut c_void` so the C header
/// stays type-agnostic).
///
/// - `on_back`: C callback fired when the back button is tapped (may be null).
/// - `ctx`: context pointer passed through to `on_back` (may be null).
///
/// The returned pointer is a **+1 retained** `UIViewController` that the
/// caller owns. Release via `bt_rs_terminal_release_vc`.
///
/// # Safety
/// `on_back` and `ctx` are stored and invoked on the main thread only.
#[no_mangle]
pub unsafe extern "C" fn bt_rs_terminal_create_vc(
    on_back: Option<BtRsBackCallback>,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    #[cfg(target_os = "ios")]
    {
        ios::create_vc(on_back, ctx)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (on_back, ctx);
        std::ptr::null_mut()
    }
}

/// Release a `UIViewController *` previously returned by
/// `bt_rs_terminal_create_vc`. Safe to call with null.
///
/// # Safety
/// `vc_ptr` must be a pointer returned by `bt_rs_terminal_create_vc` and
/// not yet released.
#[no_mangle]
pub unsafe extern "C" fn bt_rs_terminal_release_vc(vc_ptr: *mut std::ffi::c_void) {
    #[cfg(target_os = "ios")]
    {
        ios::release_vc(vc_ptr);
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = vc_ptr;
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use super::BtRsBackCallback;
    use objc2::encode::{Encode, Encoding, RefEncode};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{declare_class, msg_send, msg_send_id, sel, ClassType, DeclaredClass};
    use objc2_foundation::{MainThreadMarker, NSNotificationCenter, NSString};
    use objc2_ui_kit::{
        UIBarButtonItem, UIBarButtonItemStyle, UIColor, UIFont, UINavigationItem, UITextView,
        UIViewController,
    };
    use std::cell::{Cell, RefCell};

    /// CoreGraphics primitive types. On iOS (64-bit) `CGFloat` is `f64`.
    /// Defined locally to keep objc2-foundation feature set minimal.
    pub(super) type CGFloat = f64;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub(super) struct CGPoint {
        pub x: CGFloat,
        pub y: CGFloat,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub(super) struct CGSize {
        pub width: CGFloat,
        pub height: CGFloat,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub(super) struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub(super) struct UIEdgeInsets {
        pub top: CGFloat,
        pub left: CGFloat,
        pub bottom: CGFloat,
        pub right: CGFloat,
    }

    // SAFETY: All geometry structs are `#[repr(C)]` with matching ObjC layout.
    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding =
            Encoding::Struct("CGPoint", &[CGFloat::ENCODING, CGFloat::ENCODING]);
    }
    unsafe impl RefEncode for CGPoint {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
    }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding =
            Encoding::Struct("CGSize", &[CGFloat::ENCODING, CGFloat::ENCODING]);
    }
    unsafe impl RefEncode for CGSize {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
    }
    unsafe impl Encode for CGRect {
        const ENCODING: Encoding =
            Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }
    unsafe impl RefEncode for CGRect {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
    }
    unsafe impl Encode for UIEdgeInsets {
        const ENCODING: Encoding = Encoding::Struct(
            "UIEdgeInsets",
            &[
                CGFloat::ENCODING,
                CGFloat::ENCODING,
                CGFloat::ENCODING,
                CGFloat::ENCODING,
            ],
        );
    }
    unsafe impl RefEncode for UIEdgeInsets {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
    }

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
        /// Current keyboard height (0 when hidden). Updated by notifications.
        keyboard_height: Cell<CGFloat>,
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

                // ---------- Back bar button (unchanged) ----------
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
                let view1: Retained<UITextView> = unsafe {
                    msg_send_id![mtm.alloc::<UITextView>(), initWithFrame: zero]
                };
                // view2 — composer that hugs the keyboard
                let view2: Retained<UITextView> = unsafe {
                    msg_send_id![mtm.alloc::<UITextView>(), initWithFrame: zero]
                };

                // Make view2 visually distinct so the user can see the boundary.
                let secondary_bg: Retained<UIColor> =
                    unsafe { msg_send_id![UIColor::class(), secondarySystemBackgroundColor] };
                let _: () = unsafe { msg_send![&*view2, setBackgroundColor: &*secondary_bg] };

                // Set self as view2's delegate so we can react to text changes.
                let _: () = unsafe { msg_send![&*view2, setDelegate: self] };

                // Measure one-line height from the actual font + container inset.
                let font: Retained<UIFont> = unsafe { msg_send_id![&*view2, font] };
                let line_h: CGFloat = unsafe { msg_send![&*font, lineHeight] };
                let inset: UIEdgeInsets = unsafe { msg_send![&*view2, textContainerInset] };
                let one_line = line_h + inset.top + inset.bottom;
                let three_line = line_h * 3.0 + inset.top + inset.bottom;

                self.ivars().one_line_height.set(one_line);
                self.ivars().three_line_height.set(three_line);
                self.ivars().view2_height.set(one_line);
                self.ivars().keyboard_height.set(0.0);

                // Add as subviews.
                if let Some(view) = self.view() {
                    let _: () = unsafe { msg_send![&*view, addSubview: &*view1] };
                    let _: () = unsafe { msg_send![&*view, addSubview: &*view2] };
                }

                // Store strong refs.
                *self.ivars().view1.borrow_mut() = Some(view1);
                *self.ivars().view2.borrow_mut() = Some(view2);

                // Register keyboard observers.
                let center: Retained<NSNotificationCenter> =
                    unsafe { msg_send_id![NSNotificationCenter::class(), defaultCenter] };
                let show_name = NSString::from_str("UIKeyboardWillShowNotification");
                let hide_name = NSString::from_str("UIKeyboardWillHideNotification");
                let null_obj: *const AnyObject = std::ptr::null();
                let _: () = unsafe {
                    msg_send![
                        &*center,
                        addObserver: self as *const _ as *const AnyObject,
                        selector: sel!(keyboardWillShow:),
                        name: &*show_name,
                        object: null_obj,
                    ]
                };
                let _: () = unsafe {
                    msg_send![
                        &*center,
                        addObserver: self as *const _ as *const AnyObject,
                        selector: sel!(keyboardWillHide:),
                        name: &*hide_name,
                        object: null_obj,
                    ]
                };
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

                let kb = self.ivars().keyboard_height.get();
                let v2_h = self.ivars().view2_height.get();
                let width = bounds.size.width;
                let safe_top = insets.top;
                let safe_bottom = insets.bottom;

                // When the keyboard is up, ignore safe-area bottom (the keyboard
                // already covers the home indicator region).
                let bottom_offset = if kb > 0.0 { kb } else { safe_bottom };
                let v2_y = bounds.size.height - bottom_offset - v2_h;

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

            #[method(keyboardWillShow:)]
            fn keyboard_will_show(&self, notification: &NSObject) {
                // notification.userInfo
                let user_info: *mut AnyObject =
                    unsafe { msg_send![notification, userInfo] };
                if user_info.is_null() {
                    return;
                }
                let key = NSString::from_str("UIKeyboardFrameEndUserInfoKey");
                let value: *mut AnyObject =
                    unsafe { msg_send![user_info, objectForKey: &*key] };
                if value.is_null() {
                    return;
                }
                // NSValue.CGRectValue
                let kb_frame: CGRect = unsafe { msg_send![value, CGRectValue] };
                self.ivars().keyboard_height.set(kb_frame.size.height);
                self.relayout();
            }

            #[method(keyboardWillHide:)]
            fn keyboard_will_hide(&self, _notification: &NSObject) {
                self.ivars().keyboard_height.set(0.0);
                self.relayout();
            }

            #[method(viewWillDisappear:)]
            fn view_will_disappear(&self, animated: bool) {
                let _: () = unsafe { msg_send![super(self), viewWillDisappear: animated] };
                let center: Retained<NSNotificationCenter> =
                    unsafe { msg_send_id![NSNotificationCenter::class(), defaultCenter] };
                let _: () = unsafe { msg_send![&*center, removeObserver: self as *const _ as *const AnyObject] };
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

    pub(super) unsafe fn create_vc(
        on_back: Option<BtRsBackCallback>,
        ctx: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };

        // Use `mtm.alloc()` since `RsTerminalViewController` is `MainThreadOnly`
        // and therefore not `IsAllocableAnyThread`.
        let vc: Retained<RsTerminalViewController> =
            unsafe { msg_send_id![mtm.alloc::<RsTerminalViewController>(), init] };

        // Wire ivars.
        vc.ivars().on_back.set(on_back);
        vc.ivars().ctx.set(ctx);

        // Return a +1 retained raw pointer. Swift takes ownership via
        // `Unmanaged.fromOpaque(_:).takeRetainedValue()`.
        Retained::into_raw(vc) as *mut std::ffi::c_void
    }

    pub(super) unsafe fn release_vc(vc_ptr: *mut std::ffi::c_void) {
        if vc_ptr.is_null() {
            return;
        }
        // Re-acquire ownership and let it drop, releasing the +1 retain.
        let _ = unsafe { Retained::from_raw(vc_ptr as *mut RsTerminalViewController) };
    }
}
