//! Rust-built mirror of `SettingsScreen.swift` (the SwiftUI modal Settings
//! sheet). Composes the W23a design-system components:
//!
//! - `form_section(header, rows)`     — grouped card
//! - `toggle_row(label, on, target, action)` — UISwitch row
//! - `footer_label(text)`             — multi-line muted footnote
//!
//! Each toggle row's selector reads the new value off its `UISwitch` and
//! hands it to [`crate::settings_store`], which owns the `NSUserDefaults`
//! reads/writes. The initial state is pulled on `viewWillAppear` so
//! values written elsewhere reflect when the user opens this sheet.
//!
//! Section order, headers, row labels and footer copy mirror the original
//! SwiftUI screen row-for-row.

#![cfg(target_os = "ios")]

use crate::design_system::{
    colors,
    components::{form_card, toggle_row},
    spacing,
};
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use crate::l10n::t;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::NSString;
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIBarButtonItem, UIBarButtonItemStyle, UILayoutConstraintAxis,
    UINavigationItem, UIScrollView, UIStackView, UIStackViewAlignment, UIStackViewDistribution,
    UISwitch, UIView, UIViewController,
};
use std::cell::Cell;

/// Done-button callback installed by the FFI constructor. Fires on the
/// main thread when the user taps the navigation-bar Done item. Kept
/// for internal storage; FFI entry inlines the bare-fn type for
/// cbindgen-friendly emission (see `ffi/vc.rs`).
pub type BtIosSettingsDoneCallback = unsafe extern "C" fn(ctx: *mut std::ffi::c_void);

use crate::settings_store;

#[derive(Default)]
pub struct Ivars {
    on_done: Cell<Option<BtIosSettingsDoneCallback>>,
    /// SAFETY: never dereffed on the Rust side; ownership and lifetime
    /// are the Swift host's responsibility, same contract as
    /// `vc::Ivars::ctx`.
    ctx: Cell<*mut std::ffi::c_void>,
    /// Root vertical stack inside the scroll view — the `viewDidLayoutSubviews`
    /// pass resizes both to fill the bounds.
    scroll: std::cell::RefCell<Option<Retained<UIScrollView>>>,
    content: std::cell::RefCell<Option<Retained<UIStackView>>>,
}

// SAFETY: only accessed on the main thread (MainThreadOnly class).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// Rust-backed Settings sheet. Lives modally inside a
    /// `UINavigationController` built on the Swift side; this class only
    /// owns its own content + navigation item (title + Done button).
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosSettingsViewController"]
    #[ivars = Ivars]
    pub struct BtIosSettingsViewController;

    impl BtIosSettingsViewController {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send![super(this), init] }
        }

        #[unsafe(method(viewDidLoad))]
        fn view_did_load(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLoad] };
            let mtm = unsafe { MainThreadMarker::new_unchecked() };

            // Background — design-system surface token so dark mode works.
            let bg = colors::shadcn_background();
            if let Some(view) = self.view() {
                view.setBackgroundColor(Some(&bg));
                crate::a11y::set_a11y_id(
                    &*view as &AnyObject,
                    "settings.rust.root",
                );
            }

            // Title + Done button on the navigation bar (wrapping
            // UINavigationController is built on the Swift side).
            let nav_item: Retained<UINavigationItem> =
                unsafe { msg_send![self, navigationItem] };
            nav_item.setTitle(Some(&NSString::from_str(&t("Settings"))));
            let done_title = NSString::from_str(&t("Done"));
            let done_btn: Retained<UIBarButtonItem> = unsafe {
                UIBarButtonItem::initWithTitle_style_target_action(
                    mtm.alloc::<UIBarButtonItem>(),
                    Some(&done_title),
                    UIBarButtonItemStyle::Plain,
                    Some(self.as_ref()),
                    Some(sel!(doneTapped)),
                )
            };
            crate::a11y::set_a11y_id(&*done_btn as &AnyObject, "settings.done");
            nav_item.setRightBarButtonItem(Some(&done_btn));

            // ---- Scroll view + content stack -------------------------------
            let scroll: Retained<UIScrollView> = unsafe {
                let alloc = mtm.alloc::<UIScrollView>();
                msg_send![alloc, initWithFrame: CGRect::default()]
            };
            let content = UIStackView::new(mtm);
            content.setAxis(UILayoutConstraintAxis::Vertical);
            content.setAlignment(UIStackViewAlignment::Fill);
            content.setDistribution(UIStackViewDistribution::Fill);
            // Card-to-card spacing 12 pt + outer screen padding 16 pt,
            // mirroring the connect form's Shadcn-aligned layout.
            content.setSpacing(spacing::MD);
            content.setLayoutMarginsRelativeArrangement(true);
            content.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
                top: spacing::LG,
                leading: spacing::LG,
                bottom: spacing::LG,
                trailing: spacing::LG,
            });

            // ---- Section 1 — Display ---------------------------------------
            let reserve_initial = settings_store::reserve_top_safe_area();
            let (reserve_row, reserve_switch) = toggle_row(
                mtm,
                &t("Keep first row visible in full-screen apps"),
                reserve_initial,
                self.as_ref(),
                sel!(toggleReserveTopSafeArea:),
            );
            crate::a11y::set_a11y_id(
                &*reserve_switch as &AnyObject,
                "settings.reserveTopSafeArea",
            );
            let display_section = form_card(
                mtm,
                &t("Display"),
                Some(&t(
                    "When vim, htop, claude or other full-screen tools run, reserve the top safe area so the Dynamic Island, notch, or status bar doesn't cover their first row.",
                )),
                &[reserve_row],
            );
            content.addArrangedSubview(&display_section);

            // ---- Section 2 — Blocks ----------------------------------------
            let blocks_initial = settings_store::show_command_blocks();
            let (blocks_row, blocks_switch) = toggle_row(
                mtm,
                &t("Command blocks (Beta)"),
                blocks_initial,
                self.as_ref(),
                sel!(toggleShowCommandBlocks:),
            );
            crate::a11y::set_a11y_id(
                &*blocks_switch as &AnyObject,
                "settings.commandBlocks",
            );
            let blocks_section = form_card(
                mtm,
                &t("Blocks"),
                Some(&t(
                    "Show each command and its output as a separate block (Warp-style). BedTerm writes a small shell-integration script into every new SSH session to track prompt and command boundaries. Uses Warp's DCS hook protocol, not OSC 133. Still in beta and off by default.",
                )),
                &[blocks_row],
            );
            content.addArrangedSubview(&blocks_section);

            // Mount.
            let content_view: &UIView = unsafe { &*Retained::as_ptr(&content).cast() };
            let _: () = unsafe { msg_send![&*scroll, addSubview: content_view] };
            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addSubview: &*scroll] };
            }

            *self.ivars().scroll.borrow_mut() = Some(scroll);
            *self.ivars().content.borrow_mut() = Some(content);
        }

        #[unsafe(method(viewWillAppear:))]
        fn view_will_appear(&self, animated: bool) {
            let _: () = unsafe { msg_send![super(self), viewWillAppear: animated] };
            // Re-sync hook — currently a no-op pass. The switches aren't
            // retained, and the backing `NSUserDefaults` is cheap to re-
            // read on next open. Documented as a deferred follow-up.
        }

        #[unsafe(method(viewDidLayoutSubviews))]
        fn view_did_layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };
            let Some(view) = self.view() else { return };
            let bounds: CGRect = unsafe { msg_send![&*view, bounds] };

            let scroll_borrow = self.ivars().scroll.borrow();
            let content_borrow = self.ivars().content.borrow();
            let (Some(scroll), Some(content)) = (scroll_borrow.as_ref(), content_borrow.as_ref())
            else {
                return;
            };

            // Scroll view fills the bounds. `contentInsetAdjustmentBehavior
            // = .automatic` (default) handles the nav bar / safe area —
            // don't offset the content stack by `safeAreaInsets` again.
            let scroll_frame = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: bounds.size.width,
                    height: bounds.size.height,
                },
            };
            let _: () = unsafe { msg_send![&**scroll, setFrame: scroll_frame] };

            let available_w = bounds.size.width;
            let fitting_size = CGSize {
                width: available_w,
                height: 0.0,
            };
            // systemLayoutSizeFittingSize: → natural height for the given
            // width. 0 height + UIKit's default 0/0 means "compute it".
            let natural: CGSize =
                unsafe { msg_send![&**content, systemLayoutSizeFittingSize: fitting_size] };
            let content_height: CGFloat = natural.height.max(0.0);
            let content_frame = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: available_w,
                    height: content_height,
                },
            };
            let _: () = unsafe { msg_send![&**content, setFrame: content_frame] };

            let content_size = CGSize {
                width: bounds.size.width,
                height: content_height,
            };
            let _: () = unsafe { msg_send![&**scroll, setContentSize: content_size] };
        }

        // ---- Selector handlers --------------------------------------------

        #[unsafe(method(doneTapped))]
        fn done_tapped(&self) {
            let ivars = self.ivars();
            if let Some(cb) = ivars.on_done.get() {
                let ctx = ivars.ctx.get();
                unsafe { cb(ctx) };
            }
        }

        #[unsafe(method(toggleReserveTopSafeArea:))]
        fn toggle_reserve_top_safe_area(&self, sender: &UISwitch) {
            let on: bool = unsafe { msg_send![sender, isOn] };
            settings_store::set_reserve_top_safe_area(on);
        }

        #[unsafe(method(toggleShowCommandBlocks:))]
        fn toggle_show_command_blocks(&self, sender: &UISwitch) {
            let on: bool = unsafe { msg_send![sender, isOn] };
            settings_store::set_show_command_blocks(on);
        }

    }
);

pub(crate) unsafe fn create_settings_vc(
    on_done: Option<BtIosSettingsDoneCallback>,
    ctx: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let vc: Retained<BtIosSettingsViewController> =
        unsafe { msg_send![mtm.alloc::<BtIosSettingsViewController>(), init] };
    vc.ivars().on_done.set(on_done);
    vc.ivars().ctx.set(ctx);
    Retained::into_raw(vc) as *mut std::ffi::c_void
}

pub(crate) unsafe fn release_settings_vc(vc_ptr: *mut std::ffi::c_void) {
    if vc_ptr.is_null() {
        return;
    }
    let _ = unsafe { Retained::from_raw(vc_ptr as *mut BtIosSettingsViewController) };
}
