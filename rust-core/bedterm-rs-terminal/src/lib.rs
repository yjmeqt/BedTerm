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
    use objc2::rc::Retained;
    use objc2::{declare_class, msg_send, msg_send_id, sel, ClassType, DeclaredClass};
    use objc2_foundation::{MainThreadMarker, NSString};
    use objc2_ui_kit::{
        UIBarButtonItem, UIBarButtonItemStyle, UIColor, UINavigationItem, UIViewController,
    };
    use std::cell::Cell;

    /// Per-instance state stored as ivars.
    pub struct Ivars {
        /// C callback to fire on back-tap (may be None).
        on_back: Cell<Option<BtRsBackCallback>>,
        /// Context pointer for the callback. Only accessed on the main thread.
        ctx: Cell<*mut std::ffi::c_void>,
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
                // `systemBackgroundColor` is in the UIInterface feature of objc2-ui-kit
                // so we call it through msg_send! to keep feature dependencies minimal.
                let bg: Retained<UIColor> =
                    unsafe { msg_send_id![UIColor::class(), systemBackgroundColor] };
                if let Some(view) = self.view() {
                    view.setBackgroundColor(Some(&bg));
                }

                // Back bar button item.
                // UIBarButtonItem is MainThreadOnly so use mtm.alloc().
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
            }

            #[method(backButtonTapped)]
            fn back_button_tapped(&self) {
                let ivars = self.ivars();
                if let Some(cb) = ivars.on_back.get() {
                    let ctx = ivars.ctx.get();
                    unsafe { cb(ctx) };
                }
            }
        }
    );

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
