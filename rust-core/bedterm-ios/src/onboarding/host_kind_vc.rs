//! `BtIosOnboardingHostKindVC` — first onboarding step (R10 step 1).
//!
//! Mirrors `OnboardingHostKindStep` in
//! `BedTermKit/Sources/BedTermKit/Features/Onboarding/OnboardingScreen.swift`.
//! Two choice buttons: macOS (choice=0) vs Linux / other (choice=1). Fires
//! `on_choice(ctx, choice)` exactly once per tap; Swift coordinator drives
//! the actual step machine.

use crate::a11y;
use crate::design_system::{colors, spacing, typography};
use crate::onboarding::choice_button::make_choice_button;
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

// SAFETY: only ever touched on the main thread (MainThreadOnly).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// TODO(localization): copy is hardcoded English; Swift owns localization
    /// today. A future pass will route strings through `bt_loc(...)`.
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosOnboardingHostKindVC"]
    #[ivars = Ivars]
    pub struct BtIosOnboardingHostKindVC;

    impl BtIosOnboardingHostKindVC {
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
                a11y::set_a11y_id(&*view as &AnyObject, "onboarding.hostKind.root");
            }

            // Title — "Welcome to BedTerm".
            let title = UILabel::new(mtm);
            title.setText(Some(&NSString::from_str(&t("Welcome to BedTerm"))));
            unsafe {
                title.setFont(Some(&typography::system(34.0, typography::WEIGHT_BOLD)));
                title.setTextColor(Some(&colors::shadcn_primary()));
            }
            title.setNumberOfLines(0);
            let _: () = unsafe { msg_send![&*title, setTextAlignment: 1_i64] }; // .center

            // Body — "What kind of host will you connect to?".
            let body = UILabel::new(mtm);
            body.setText(Some(&NSString::from_str(&t(
                "What kind of host will you connect to?",
            ))));
            unsafe {
                body.setFont(Some(&typography::body()));
                body.setTextColor(Some(&colors::shadcn_muted_foreground()));
            }
            body.setNumberOfLines(0);
            let _: () = unsafe { msg_send![&*body, setTextAlignment: 1_i64] };

            // Choice buttons.
            let macos_btn = make_choice_button(
                mtm,
                "macOS",
                &t("A Mac mini, MacBook, or iMac on your network"),
                self.as_ref(),
                sel!(choiceMacOSTapped),
            );
            a11y::set_a11y_id(&*macos_btn as &AnyObject, "onboarding.hostKind.macos");

            let other_btn = make_choice_button(
                mtm,
                &t("Linux / other"),
                &t("Linux box, VM, Raspberry Pi, cloud server"),
                self.as_ref(),
                sel!(choiceOtherTapped),
            );
            a11y::set_a11y_id(&*other_btn as &AnyObject, "onboarding.hostKind.other");

            // Buttons stack (vertical 12pt gap).
            let buttons_stack = UIStackView::new(mtm);
            buttons_stack.setAxis(UILayoutConstraintAxis::Vertical);
            buttons_stack.setAlignment(UIStackViewAlignment::Fill);
            buttons_stack.setDistribution(UIStackViewDistribution::Fill);
            buttons_stack.setSpacing(spacing::MD);
            buttons_stack
                .addArrangedSubview(unsafe { &*(&*macos_btn as *const _ as *const UIView) });
            buttons_stack
                .addArrangedSubview(unsafe { &*(&*other_btn as *const _ as *const UIView) });

            // Outer vertical stack: spacer, title, body, buttons, spacer.
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
                let _: () = unsafe {
                    msg_send![&*stack, setTranslatesAutoresizingMaskIntoConstraints: false]
                };
                view.addSubview(unsafe { &*(&*stack as *const UIStackView as *const UIView) });
                pin_to_safe_area(&stack, &view);

                // Nav title.
                let nav_item: Retained<AnyObject> = unsafe { msg_send![self, navigationItem] };
                let title_ns = NSString::from_str(&t("Setup"));
                let _: () = unsafe { msg_send![&*nav_item, setTitle: &*title_ns] };
            }
        }

        #[unsafe(method(choiceMacOSTapped))]
        fn choice_macos_tapped(&self) {
            self.fire_choice(0);
        }

        #[unsafe(method(choiceOtherTapped))]
        fn choice_other_tapped(&self) {
            self.fire_choice(1);
        }
    }
);

impl BtIosOnboardingHostKindVC {
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

/// Pin a child view to the parent's safe area on all four sides.
pub(crate) fn pin_to_safe_area(child: &UIStackView, parent: &UIView) {
    let _: () = unsafe { msg_send![child, setTranslatesAutoresizingMaskIntoConstraints: false] };
    unsafe {
        let safe: Retained<AnyObject> = msg_send![parent, safeAreaLayoutGuide];
        let child_top: Retained<AnyObject> = msg_send![child, topAnchor];
        let child_bottom: Retained<AnyObject> = msg_send![child, bottomAnchor];
        let child_lead: Retained<AnyObject> = msg_send![child, leadingAnchor];
        let child_trail: Retained<AnyObject> = msg_send![child, trailingAnchor];
        let p_top: Retained<AnyObject> = msg_send![&*safe, topAnchor];
        let p_bottom: Retained<AnyObject> = msg_send![&*safe, bottomAnchor];
        let p_lead: Retained<AnyObject> = msg_send![&*safe, leadingAnchor];
        let p_trail: Retained<AnyObject> = msg_send![&*safe, trailingAnchor];
        let c1: Retained<AnyObject> = msg_send![&*child_top, constraintEqualToAnchor: &*p_top];
        let c2: Retained<AnyObject> =
            msg_send![&*child_bottom, constraintEqualToAnchor: &*p_bottom];
        let c3: Retained<AnyObject> = msg_send![&*child_lead, constraintEqualToAnchor: &*p_lead];
        let c4: Retained<AnyObject> = msg_send![&*child_trail, constraintEqualToAnchor: &*p_trail];
        let _: () = msg_send![&*c1, setActive: true];
        let _: () = msg_send![&*c2, setActive: true];
        let _: () = msg_send![&*c3, setActive: true];
        let _: () = msg_send![&*c4, setActive: true];
    }
}

pub(crate) unsafe fn create_host_kind_vc(
    on_choice: Option<BtIosOnboardingChoiceCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let vc: Retained<BtIosOnboardingHostKindVC> =
        unsafe { msg_send![mtm.alloc::<BtIosOnboardingHostKindVC>(), init] };
    vc.set_callback(on_choice, ctx);
    Retained::into_raw(vc) as *mut c_void
}
