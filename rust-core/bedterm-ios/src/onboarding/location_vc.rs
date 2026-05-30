//! `BtIosOnboardingLocationVC` — second onboarding step (R10 step 2).
//!
//! Mirrors `OnboardingLocationStep`. Two choices: same-Wi-Fi (0) vs remote (1).

#![cfg(target_os = "ios")]

use crate::a11y;
use crate::design_system::{colors, spacing, typography};
use crate::onboarding::choice_button::make_choice_button;
use crate::onboarding::host_kind_vc::pin_to_safe_area;
use crate::onboarding::BtIosOnboardingChoiceCallback;
use bedterm_app::l10n::t;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UILabel, UILayoutConstraintAxis, UIStackView, UIStackViewAlignment,
    UIStackViewDistribution, UIView, UIViewController,
};
use std::cell::Cell;
use std::ffi::c_void;

#[derive(Default)]
pub struct Ivars {
    on_choice: Cell<Option<BtIosOnboardingChoiceCallback>>,
    ctx: Cell<*mut c_void>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// TODO(localization): copy is hardcoded English; Swift owns localization
    /// today.
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosOnboardingLocationVC"]
    #[ivars = Ivars]
    pub struct BtIosOnboardingLocationVC;

    impl BtIosOnboardingLocationVC {
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
                a11y::set_a11y_id(&*view as &AnyObject, "onboarding.location.root");
            }

            let title = UILabel::new(mtm);
            title.setText(Some(&NSString::from_str(&t("Where is the host?"))));
            unsafe {
                title.setFont(Some(&typography::system(28.0, typography::WEIGHT_BOLD)));
                title.setTextColor(Some(&colors::shadcn_primary()));
            }
            title.setNumberOfLines(0);
            let _: () = unsafe { msg_send![&*title, setTextAlignment: 1_i64] };

            let body = UILabel::new(mtm);
            body.setText(Some(&NSString::from_str(&t(
                "This determines whether we need Local Network access.",
            ))));
            unsafe {
                body.setFont(Some(&typography::callout()));
                body.setTextColor(Some(&colors::shadcn_muted_foreground()));
            }
            body.setNumberOfLines(0);
            let _: () = unsafe { msg_send![&*body, setTextAlignment: 1_i64] };

            let same_btn = make_choice_button(
                mtm,
                &t("Same Wi-Fi as my phone"),
                &t("iOS will ask for Local Network permission"),
                self.as_ref(),
                sel!(choiceSameWifiTapped),
            );
            a11y::set_a11y_id(&*same_btn as &AnyObject, "onboarding.location.sameWifi");

            let remote_btn = make_choice_button(
                mtm,
                &t("Remote (over the internet)"),
                &t("A public IP or hostname reachable from anywhere"),
                self.as_ref(),
                sel!(choiceRemoteTapped),
            );
            a11y::set_a11y_id(&*remote_btn as &AnyObject, "onboarding.location.remote");

            let buttons_stack = UIStackView::new(mtm);
            buttons_stack.setAxis(UILayoutConstraintAxis::Vertical);
            buttons_stack.setAlignment(UIStackViewAlignment::Fill);
            buttons_stack.setDistribution(UIStackViewDistribution::Fill);
            buttons_stack.setSpacing(spacing::MD);
            buttons_stack
                .addArrangedSubview(unsafe { &*(&*same_btn as *const _ as *const UIView) });
            buttons_stack
                .addArrangedSubview(unsafe { &*(&*remote_btn as *const _ as *const UIView) });

            let stack = UIStackView::new(mtm);
            stack.setAxis(UILayoutConstraintAxis::Vertical);
            stack.setAlignment(UIStackViewAlignment::Fill);
            stack.setDistribution(UIStackViewDistribution::Fill);
            stack.setSpacing(spacing::XL);
            stack.setLayoutMarginsRelativeArrangement(true);
            stack.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
                top: spacing::XL,
                leading: spacing::LG,
                bottom: spacing::XL,
                trailing: spacing::LG,
            });
            stack.addArrangedSubview(unsafe { &*(&*title as *const UILabel as *const UIView) });
            stack.addArrangedSubview(unsafe { &*(&*body as *const UILabel as *const UIView) });
            stack.addArrangedSubview(unsafe {
                &*(&*buttons_stack as *const UIStackView as *const UIView)
            });

            if let Some(view) = self.view() {
                view.addSubview(unsafe { &*(&*stack as *const UIStackView as *const UIView) });
                pin_to_safe_area(&stack, &view);

                let nav_item: Retained<AnyObject> = unsafe { msg_send![self, navigationItem] };
                let title_ns = NSString::from_str(&t("Where"));
                let _: () = unsafe { msg_send![&*nav_item, setTitle: &*title_ns] };
            }
        }

        #[unsafe(method(choiceSameWifiTapped))]
        fn choice_same_tapped(&self) {
            self.fire_choice(0);
        }

        #[unsafe(method(choiceRemoteTapped))]
        fn choice_remote_tapped(&self) {
            self.fire_choice(1);
        }
    }
);

impl BtIosOnboardingLocationVC {
    fn fire_choice(&self, choice: i32) {
        if let Some(cb) = self.ivars().on_choice.get() {
            let ctx = self.ivars().ctx.get();
            unsafe { cb(ctx, choice) };
        }
    }

    pub(crate) fn set_callback(
        &self,
        on_choice: Option<BtIosOnboardingChoiceCallback>,
        ctx: *mut c_void,
    ) {
        self.ivars().on_choice.set(on_choice);
        self.ivars().ctx.set(ctx);
    }
}

pub(crate) unsafe fn create_location_vc(
    on_choice: Option<BtIosOnboardingChoiceCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let vc: Retained<BtIosOnboardingLocationVC> =
        unsafe { msg_send![mtm.alloc::<BtIosOnboardingLocationVC>(), init] };
    vc.set_callback(on_choice, ctx);
    Retained::into_raw(vc) as *mut c_void
}
