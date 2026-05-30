#![cfg(target_os = "ios")]

use crate::design_system::colors;
use crate::design_system::components::primary_button;
use crate::l10n::t;
use objc2::rc::{Allocated, Retained};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::NSString;
use objc2_ui_kit::{
    UILabel, UILayoutConstraintAxis, UIStackView, UIStackViewAlignment, UIStackViewDistribution,
    UIViewController,
};
use std::cell::Cell;
use std::ffi::c_void;

pub type MismatchCallback = Option<unsafe extern "C" fn(*mut c_void)>;

#[derive(Default)]
pub struct Ivars {
    on_trust: Cell<MismatchCallback>,
    on_reject: Cell<MismatchCallback>,
    ctx: Cell<*mut c_void>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosHostKeyMismatchViewController"]
    #[ivars = Ivars]
    pub struct BtIosHostKeyMismatchViewController;

    impl BtIosHostKeyMismatchViewController {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send![super(this), init] }
        }

        #[unsafe(method(viewDidLoad))]
        fn view_did_load(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLoad] };
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            if let Some(view) = self.view() {
                view.setBackgroundColor(Some(&colors::shadcn_background()));
            }

            let root = UIStackView::new(mtm);
            root.setAxis(UILayoutConstraintAxis::Vertical);
            root.setAlignment(UIStackViewAlignment::Center);
            root.setDistribution(UIStackViewDistribution::Fill);
            root.setSpacing(20.0);

            let title = UILabel::new(mtm);
            title.setText(Some(&NSString::from_str(&t("Host key changed"))));
            root.addArrangedSubview(&title);

            // Use primary_button from design system.
            let trust_btn = primary_button(
                mtm, "checkmark.shield", "Trust New Key",
                self.as_ref(), sel!(trustTapped));
            root.addArrangedSubview(&trust_btn);

            let reject_btn = primary_button(
                mtm, "xmark.shield", "Reject",
                self.as_ref(), sel!(rejectTapped));
            root.addArrangedSubview(&reject_btn);

            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addSubview: &*root] };
            }
            root.setTranslatesAutoresizingMaskIntoConstraints(false);
        }

        #[unsafe(method(trustTapped))]
        fn trust_tapped(&self) {
            if let Some(cb) = self.ivars().on_trust.get() {
                unsafe { cb(self.ivars().ctx.get()) };
            }
        }

        #[unsafe(method(rejectTapped))]
        fn reject_tapped(&self) {
            if let Some(cb) = self.ivars().on_reject.get() {
                unsafe { cb(self.ivars().ctx.get()) };
            }
        }
    }
);

impl BtIosHostKeyMismatchViewController {
    pub(crate) fn set_callbacks(
        &self,
        on_trust: MismatchCallback,
        on_reject: MismatchCallback,
        ctx: *mut c_void,
    ) {
        self.ivars().on_trust.set(on_trust);
        self.ivars().on_reject.set(on_reject);
        self.ivars().ctx.set(ctx);
    }
}

pub(crate) unsafe fn create_mismatch_vc(
    on_trust: MismatchCallback,
    on_reject: MismatchCallback,
    ctx: *mut c_void,
) -> *mut c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let vc: Retained<BtIosHostKeyMismatchViewController> =
        unsafe { msg_send![mtm.alloc::<BtIosHostKeyMismatchViewController>(), init] };
    vc.set_callbacks(on_trust, on_reject, ctx);
    Retained::into_raw(vc) as *mut c_void
}

pub(crate) unsafe fn release_mismatch_vc(vc_ptr: *mut c_void) {
    if vc_ptr.is_null() {
        return;
    }
    drop(unsafe { Retained::<BtIosHostKeyMismatchViewController>::retain(vc_ptr as *mut _) });
}
