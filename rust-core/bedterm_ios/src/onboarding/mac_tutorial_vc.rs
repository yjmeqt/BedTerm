//! `BtIosOnboardingMacTutorialVC` — third onboarding step (R10 step 3a).
//!
//! Mirrors `OnboardingMacTutorialStep`. The SwiftUI version is a 3-page
//! `TabView`; for the Rust port we render all three sections in a single
//! scroll view (the paging chrome is deferred — the underlying copy and
//! ordering is preserved). A bottom-aligned primary "Continue" button fires
//! the single `on_continue` callback.

#![cfg(target_os = "ios")]

use crate::a11y;
use crate::design_system::colors;
use crate::design_system::components::primary_button;
use crate::design_system::{spacing, typography};
use crate::onboarding::BtIosOnboardingContinueCallback;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UILabel, UILayoutConstraintAxis, UIScrollView, UIStackView,
    UIStackViewAlignment, UIStackViewDistribution, UIView, UIViewController,
};
use std::cell::Cell;
use std::ffi::c_void;

#[derive(Default)]
pub struct Ivars {
    on_continue: Cell<Option<BtIosOnboardingContinueCallback>>,
    ctx: Cell<*mut c_void>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// TODO(localization): copy is hardcoded English; Swift owns localization
    /// today.
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosOnboardingMacTutorialVC"]
    #[ivars = Ivars]
    pub struct BtIosOnboardingMacTutorialVC;

    impl BtIosOnboardingMacTutorialVC {
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
                a11y::set_a11y_id(&*view as &AnyObject, "onboarding.macTutorial.root");
            }

            // Inner content stack with the three sections.
            let content = UIStackView::new(mtm);
            content.setAxis(UILayoutConstraintAxis::Vertical);
            content.setAlignment(UIStackViewAlignment::Fill);
            content.setDistribution(UIStackViewDistribution::Fill);
            content.setSpacing(spacing::XL);
            content.setLayoutMarginsRelativeArrangement(true);
            content.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
                top: spacing::XL,
                leading: spacing::LG,
                bottom: spacing::LG,
                trailing: spacing::LG,
            });

            content.addArrangedSubview(unsafe {
                &*(&*tutorial_section(
                    mtm,
                    "1",
                    "Enable Remote Login",
                    "On your Mac, open System Settings \u{2192} General \u{2192} Sharing, then turn on Remote Login.",
                ) as *const UIStackView as *const UIView)
            });
            content.addArrangedSubview(unsafe {
                &*(&*tutorial_section(
                    mtm,
                    "2",
                    "Find your Mac's IP address",
                    "Open System Settings \u{2192} Network \u{2192} Wi-Fi \u{2192} Details. Copy the IP address \u{2014} you'll enter it on the next screen.",
                ) as *const UIStackView as *const UIView)
            });
            content.addArrangedSubview(unsafe {
                &*(&*tutorial_section(
                    mtm,
                    "3",
                    "Prevent your Mac from sleeping",
                    "A sleeping Mac silently drops SSH connections. Your display can sleep \u{2014} the system must stay awake.\n\nSystem Settings \u{2192} Battery \u{2192} Options \u{2192} turn on \"Prevent automatic sleeping on power adapter when the display is off\".\n\nOr run this in Terminal: sudo pmset -a sleep 0\n\nTo undo, return to the same Battery setting, or run: sudo pmset -a sleep 1",
                ) as *const UIStackView as *const UIView)
            });

            // Scroll view.
            let scroll = UIScrollView::new(mtm);
            scroll.addSubview(unsafe {
                &*(&*content as *const UIStackView as *const UIView)
            });

            // Continue button.
            let continue_btn = primary_button(
                mtm,
                "arrow.forward",
                "Continue",
                self.as_ref(),
                sel!(continueTapped),
            );
            a11y::set_a11y_id(&*continue_btn as &AnyObject, "onboarding.macTutorial.continue");

            // Bottom bar wrapping the continue button.
            let bottom_bar = UIView::new(mtm);
            bottom_bar.setBackgroundColor(Some(&colors::shadcn_background()));
            bottom_bar.addSubview(unsafe { &*(&*continue_btn as *const _ as *const UIView) });

            if let Some(view) = self.view() {
                view.addSubview(unsafe { &*(&*scroll as *const UIScrollView as *const UIView) });
                view.addSubview(unsafe { &*(&*bottom_bar as *const UIView) });

                let _: () = unsafe {
                    msg_send![&*scroll, setTranslatesAutoresizingMaskIntoConstraints: false]
                };
                let _: () = unsafe {
                    msg_send![&*content, setTranslatesAutoresizingMaskIntoConstraints: false]
                };
                let _: () = unsafe {
                    msg_send![&*bottom_bar, setTranslatesAutoresizingMaskIntoConstraints: false]
                };
                let _: () = unsafe {
                    msg_send![
                        &*continue_btn,
                        setTranslatesAutoresizingMaskIntoConstraints: false
                    ]
                };

                pin_scroll_layout(&scroll, &content, &bottom_bar, &continue_btn, &view);

                let nav_item: Retained<AnyObject> = unsafe { msg_send![self, navigationItem] };
                let title_ns = NSString::from_str("macOS Setup");
                let _: () = unsafe { msg_send![&*nav_item, setTitle: &*title_ns] };
            }
        }

        #[unsafe(method(continueTapped))]
        fn continue_tapped(&self) {
            if let Some(cb) = self.ivars().on_continue.get() {
                let ctx = self.ivars().ctx.get();
                unsafe { cb(ctx) };
            }
        }
    }
);

impl BtIosOnboardingMacTutorialVC {
    pub(crate) fn set_callback(
        &self,
        on_continue: Option<BtIosOnboardingContinueCallback>,
        ctx: *mut c_void,
    ) {
        self.ivars().on_continue.set(on_continue);
        self.ivars().ctx.set(ctx);
    }
}

fn tutorial_section(
    mtm: MainThreadMarker,
    number: &str,
    title: &str,
    body: &str,
) -> Retained<UIStackView> {
    let number_label = UILabel::new(mtm);
    number_label.setText(Some(&NSString::from_str(number)));
    unsafe {
        number_label.setFont(Some(&typography::system(22.0, typography::WEIGHT_BOLD)));
        number_label.setTextColor(Some(&colors::shadcn_primary()));
    }
    let _: () = unsafe { msg_send![&*number_label, setTextAlignment: 1_i64] };

    let title_label = UILabel::new(mtm);
    title_label.setText(Some(&NSString::from_str(title)));
    unsafe {
        title_label.setFont(Some(&typography::headline()));
        title_label.setTextColor(Some(&colors::shadcn_primary()));
    }
    title_label.setNumberOfLines(0);

    let body_label = UILabel::new(mtm);
    body_label.setText(Some(&NSString::from_str(body)));
    unsafe {
        body_label.setFont(Some(&typography::body()));
        body_label.setTextColor(Some(&colors::shadcn_muted_foreground()));
    }
    body_label.setNumberOfLines(0);

    let text_stack = UIStackView::new(mtm);
    text_stack.setAxis(UILayoutConstraintAxis::Vertical);
    text_stack.setAlignment(UIStackViewAlignment::Fill);
    text_stack.setDistribution(UIStackViewDistribution::Fill);
    text_stack.setSpacing(spacing::SM);
    text_stack.addArrangedSubview(unsafe { &*(&*title_label as *const UILabel as *const UIView) });
    text_stack.addArrangedSubview(unsafe { &*(&*body_label as *const UILabel as *const UIView) });

    let row = UIStackView::new(mtm);
    row.setAxis(UILayoutConstraintAxis::Horizontal);
    row.setAlignment(UIStackViewAlignment::Top);
    row.setDistribution(UIStackViewDistribution::Fill);
    row.setSpacing(spacing::MD);
    row.addArrangedSubview(unsafe { &*(&*number_label as *const UILabel as *const UIView) });
    row.addArrangedSubview(unsafe { &*(&*text_stack as *const UIStackView as *const UIView) });

    // Number label fixed width 28.
    unsafe {
        let anchor: Retained<AnyObject> = msg_send![&*number_label, widthAnchor];
        let c: Retained<AnyObject> = msg_send![&*anchor, constraintEqualToConstant: 28.0_f64];
        let _: () = msg_send![&*c, setActive: true];
    }

    row
}

fn pin_scroll_layout(
    scroll: &UIScrollView,
    content: &UIStackView,
    bottom_bar: &UIView,
    continue_btn: &objc2_ui_kit::UIButton,
    parent: &UIView,
) {
    unsafe {
        let safe: Retained<AnyObject> = msg_send![parent, safeAreaLayoutGuide];
        let s_top: Retained<AnyObject> = msg_send![&*safe, topAnchor];
        let s_lead: Retained<AnyObject> = msg_send![&*safe, leadingAnchor];
        let s_trail: Retained<AnyObject> = msg_send![&*safe, trailingAnchor];
        let s_bottom: Retained<AnyObject> = msg_send![&*safe, bottomAnchor];

        // scroll top → safe top, leading/trailing → safe.
        let scr_top: Retained<AnyObject> = msg_send![scroll, topAnchor];
        let scr_lead: Retained<AnyObject> = msg_send![scroll, leadingAnchor];
        let scr_trail: Retained<AnyObject> = msg_send![scroll, trailingAnchor];
        let scr_bot: Retained<AnyObject> = msg_send![scroll, bottomAnchor];
        let bar_top: Retained<AnyObject> = msg_send![bottom_bar, topAnchor];
        let bar_lead: Retained<AnyObject> = msg_send![bottom_bar, leadingAnchor];
        let bar_trail: Retained<AnyObject> = msg_send![bottom_bar, trailingAnchor];
        let bar_bot: Retained<AnyObject> = msg_send![bottom_bar, bottomAnchor];

        let arr: [Retained<AnyObject>; 7] = [
            msg_send![&*scr_top, constraintEqualToAnchor: &*s_top],
            msg_send![&*scr_lead, constraintEqualToAnchor: &*s_lead],
            msg_send![&*scr_trail, constraintEqualToAnchor: &*s_trail],
            msg_send![&*scr_bot, constraintEqualToAnchor: &*bar_top],
            msg_send![&*bar_lead, constraintEqualToAnchor: &*s_lead],
            msg_send![&*bar_trail, constraintEqualToAnchor: &*s_trail],
            msg_send![&*bar_bot, constraintEqualToAnchor: &*s_bottom],
        ];
        for c in arr.iter() {
            let _: () = msg_send![&**c, setActive: true];
        }

        // content inside scroll: pin to scroll's contentLayoutGuide on all
        // edges, and width to scroll's frameLayoutGuide.
        let content_guide: Retained<AnyObject> = msg_send![scroll, contentLayoutGuide];
        let frame_guide: Retained<AnyObject> = msg_send![scroll, frameLayoutGuide];
        let c_top: Retained<AnyObject> = msg_send![content, topAnchor];
        let c_lead: Retained<AnyObject> = msg_send![content, leadingAnchor];
        let c_trail: Retained<AnyObject> = msg_send![content, trailingAnchor];
        let c_bot: Retained<AnyObject> = msg_send![content, bottomAnchor];
        let g_top: Retained<AnyObject> = msg_send![&*content_guide, topAnchor];
        let g_lead: Retained<AnyObject> = msg_send![&*content_guide, leadingAnchor];
        let g_trail: Retained<AnyObject> = msg_send![&*content_guide, trailingAnchor];
        let g_bot: Retained<AnyObject> = msg_send![&*content_guide, bottomAnchor];
        let c_width: Retained<AnyObject> = msg_send![content, widthAnchor];
        let f_width: Retained<AnyObject> = msg_send![&*frame_guide, widthAnchor];

        let arr2: [Retained<AnyObject>; 5] = [
            msg_send![&*c_top, constraintEqualToAnchor: &*g_top],
            msg_send![&*c_lead, constraintEqualToAnchor: &*g_lead],
            msg_send![&*c_trail, constraintEqualToAnchor: &*g_trail],
            msg_send![&*c_bot, constraintEqualToAnchor: &*g_bot],
            msg_send![&*c_width, constraintEqualToAnchor: &*f_width],
        ];
        for c in arr2.iter() {
            let _: () = msg_send![&**c, setActive: true];
        }

        // Center continue button inside bottom bar with padding.
        let btn_lead: Retained<AnyObject> = msg_send![continue_btn, leadingAnchor];
        let btn_trail: Retained<AnyObject> = msg_send![continue_btn, trailingAnchor];
        let btn_top: Retained<AnyObject> = msg_send![continue_btn, topAnchor];
        let btn_bot: Retained<AnyObject> = msg_send![continue_btn, bottomAnchor];
        let bb_lead: Retained<AnyObject> = msg_send![bottom_bar, leadingAnchor];
        let bb_trail: Retained<AnyObject> = msg_send![bottom_bar, trailingAnchor];
        let bb_top: Retained<AnyObject> = msg_send![bottom_bar, topAnchor];
        let bb_bot: Retained<AnyObject> = msg_send![bottom_bar, bottomAnchor];

        let arr3: [Retained<AnyObject>; 4] = [
            msg_send![&*btn_lead, constraintEqualToAnchor: &*bb_lead, constant: spacing::LG],
            msg_send![&*btn_trail, constraintEqualToAnchor: &*bb_trail, constant: -spacing::LG],
            msg_send![&*btn_top, constraintEqualToAnchor: &*bb_top, constant: spacing::MD],
            msg_send![&*btn_bot, constraintEqualToAnchor: &*bb_bot, constant: -spacing::MD],
        ];
        for c in arr3.iter() {
            let _: () = msg_send![&**c, setActive: true];
        }
    }
}

pub(crate) unsafe fn create_mac_tutorial_vc(
    on_continue: Option<BtIosOnboardingContinueCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let vc: Retained<BtIosOnboardingMacTutorialVC> =
        unsafe { msg_send![mtm.alloc::<BtIosOnboardingMacTutorialVC>(), init] };
    vc.set_callback(on_continue, ctx);
    Retained::into_raw(vc) as *mut c_void
}
