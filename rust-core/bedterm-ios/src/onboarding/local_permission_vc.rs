//! `BtIosOnboardingLocalPermissionVC` — fourth onboarding step (R10 step 3b).
//!
//! Mirrors `OnboardingLocalPermissionStep`. The actual `LocalNetworkPrewarmer`
//! Bonjour probe is owned by Swift; this VC simply fires `on_continue(ctx)`
//! when the primary button is tapped, and the coordinator decides whether
//! to await `requestLocalNetworkIfNeeded` before popping. Copy is locked to
//! the "Local Network Access" / sameWifi prose by default; if the host is
//! remote the Swift coordinator can swap a different VC instance.

use crate::a11y;
use crate::design_system::colors;
use crate::design_system::components::primary_button;
use crate::design_system::{spacing, typography};
use crate::onboarding::host_kind_vc::pin_to_safe_area;
use crate::onboarding::BtIosOnboardingContinueCallback;
use bedterm_app::geometry::CGFloat;
use bedterm_app::l10n::t;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIImage, UIImageSymbolConfiguration, UIImageSymbolWeight, UIImageView,
    UILabel, UILayoutConstraintAxis, UIStackView, UIStackViewAlignment, UIStackViewDistribution,
    UIView, UIViewController,
};
use std::cell::Cell;
use std::ffi::c_void;

const WIFI_ICON_SIZE: CGFloat = 56.0;

#[derive(Default)]
pub struct Ivars {
    on_continue: Cell<Option<BtIosOnboardingContinueCallback>>,
    ctx: Cell<*mut c_void>,
    /// 0 = same-Wi-Fi copy, 1 = remote "all set" copy. Set before viewDidLoad.
    variant: Cell<i32>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// TODO(localization): copy is hardcoded English; Swift owns localization
    /// today.
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosOnboardingLocalPermissionVC"]
    #[ivars = Ivars]
    pub struct BtIosOnboardingLocalPermissionVC;

    impl BtIosOnboardingLocalPermissionVC {
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
                a11y::set_a11y_id(&*view as &AnyObject, "onboarding.localPermission.root");
            }

            let variant = self.ivars().variant.get();
            let (title_text, detail_text, button_text, nav_text): (String, String, String, String) =
                if variant == 0 {
                    (
                        t("Local Network Access"),
                        t("BedTerm needs Local Network access to reach your Mac on the same Wi-Fi. Tap Allow when iOS prompts you."),
                        t("Request Permission"),
                        t("Permission"),
                    )
                } else {
                    (
                        t("All set"),
                        t("You're ready to connect to a remote host. Tap Continue to enter the connection details."),
                        t("Continue"),
                        t("Done"),
                    )
                };

            // Wi-Fi icon.
            let icon_view = UIImageView::new(mtm);
            let icon_name = NSString::from_str("wifi");
            let cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
                WIFI_ICON_SIZE,
                UIImageSymbolWeight::Regular,
            );
            if let Some(image) = UIImage::systemImageNamed(&icon_name) {
                let configured: Retained<UIImage> =
                    unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*cfg] };
                icon_view.setImage(Some(&configured));
            }
            unsafe {
                let tint = colors::shadcn_primary();
                let _: () = msg_send![&*icon_view, setTintColor: &*tint];
            }
            let _: () = unsafe { msg_send![&*icon_view, setContentMode: 4_i64] }; // .center

            let title_label = UILabel::new(mtm);
            title_label.setText(Some(&NSString::from_str(&title_text)));
            unsafe {
                title_label.setFont(Some(&typography::system(28.0, typography::WEIGHT_BOLD)));
                title_label.setTextColor(Some(&colors::shadcn_primary()));
            }
            title_label.setNumberOfLines(0);
            let _: () = unsafe { msg_send![&*title_label, setTextAlignment: 1_i64] };

            let detail_label = UILabel::new(mtm);
            detail_label.setText(Some(&NSString::from_str(&detail_text)));
            unsafe {
                detail_label.setFont(Some(&typography::body()));
                detail_label.setTextColor(Some(&colors::shadcn_muted_foreground()));
            }
            detail_label.setNumberOfLines(0);
            let _: () = unsafe { msg_send![&*detail_label, setTextAlignment: 1_i64] };

            let action_btn = primary_button(
                mtm,
                "arrow.forward",
                &button_text,
                self.as_ref(),
                sel!(continueTapped),
            );
            a11y::set_a11y_id(&*action_btn as &AnyObject, "onboarding.localPermission.action");

            let stack = UIStackView::new(mtm);
            stack.setAxis(UILayoutConstraintAxis::Vertical);
            stack.setAlignment(UIStackViewAlignment::Fill);
            stack.setDistribution(UIStackViewDistribution::Fill);
            stack.setSpacing(spacing::LG);
            stack.setLayoutMarginsRelativeArrangement(true);
            stack.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
                top: spacing::XL,
                leading: spacing::LG,
                bottom: spacing::XL,
                trailing: spacing::LG,
            });

            stack.addArrangedSubview(unsafe {
                &*(&*icon_view as *const UIImageView as *const UIView)
            });
            stack
                .addArrangedSubview(unsafe { &*(&*title_label as *const UILabel as *const UIView) });
            stack.addArrangedSubview(unsafe {
                &*(&*detail_label as *const UILabel as *const UIView)
            });

            // Spacer-style flexible UIView pushed button down.
            let spacer = UIView::new(mtm);
            stack.addArrangedSubview(&spacer);
            unsafe {
                let _: () =
                    msg_send![&*spacer, setContentHuggingPriority: 1_f32, forAxis: 1_i64];
            }

            stack.addArrangedSubview(unsafe { &*(&*action_btn as *const _ as *const UIView) });

            if let Some(view) = self.view() {
                view.addSubview(unsafe { &*(&*stack as *const UIStackView as *const UIView) });
                pin_to_safe_area(&stack, &view);

                let nav_item: Retained<AnyObject> = unsafe { msg_send![self, navigationItem] };
                let title_ns = NSString::from_str(&nav_text);
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

impl BtIosOnboardingLocalPermissionVC {
    pub(crate) fn set_callback(
        &self,
        on_continue: Option<BtIosOnboardingContinueCallback>,
        ctx: *mut c_void,
    ) {
        self.ivars().on_continue.set(on_continue);
        self.ivars().ctx.set(ctx);
    }

    pub(crate) fn set_variant(&self, variant: i32) {
        self.ivars().variant.set(variant);
    }
}

pub(crate) unsafe fn create_local_permission_vc(
    on_continue: Option<BtIosOnboardingContinueCallback>,
    ctx: *mut c_void,
    is_remote: bool,
) -> *mut c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let vc: Retained<BtIosOnboardingLocalPermissionVC> =
        unsafe { msg_send![mtm.alloc::<BtIosOnboardingLocalPermissionVC>(), init] };
    vc.set_callback(on_continue, ctx);
    vc.set_variant(if is_remote { 1 } else { 0 });
    Retained::into_raw(vc) as *mut c_void
}
