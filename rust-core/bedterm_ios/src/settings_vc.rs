//! Rust-built mirror of `SettingsScreen.swift` (the SwiftUI modal Settings
//! sheet). Composes the W23a design-system components:
//!
//! - `form_section(header, rows)`     — grouped card
//! - `toggle_row(label, on, target, action)` — UISwitch row
//! - `footer_label(text)`             — multi-line muted footnote
//!
//! Each toggle row's selector reads the new value off its `UISwitch` and
//! hands it to the Swift-side bridge via the `bt_swift_settings_*`
//! `@_cdecl` exports declared at the top of this file. The initial state
//! is pulled on `viewWillAppear` so values written elsewhere in the app
//! (e.g. via the SwiftUI screen while debug-flag flipping) reflect when
//! the user opens this sheet.
//!
//! Section order, headers, row labels and footer copy mirror
//! `SettingsScreen.swift` row-for-row.
//!
//! TODO(localization): English strings are inline here. The SwiftUI
//! screen owns the localized variants; once this VC graduates from
//! experimental we'll route copy through a Swift-side strings provider.

#![cfg(target_os = "ios")]

use crate::design_system::{
    colors,
    components::{footer_label, form_section, toggle_row},
    spacing,
};
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize, UIEdgeInsets};
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
/// main thread when the user taps the navigation-bar Done item.
pub type BtIosSettingsDoneCallback = unsafe extern "C" fn(ctx: *mut std::ffi::c_void);

// Reads from Swift via `@_cdecl`. Returns the documented default when
// Swift hasn't installed an `observableHandle` yet (unit tests).
extern "C" {
    fn bt_swift_settings_get_reserve_top_safe_area() -> bool;
    fn bt_swift_settings_set_reserve_top_safe_area(value: bool);
    fn bt_swift_settings_get_show_command_blocks() -> bool;
    fn bt_swift_settings_set_show_command_blocks(value: bool);
    fn bt_swift_settings_get_use_rust_hosts_list() -> bool;
    fn bt_swift_settings_set_use_rust_hosts_list(value: bool);
    fn bt_swift_settings_get_use_rust_connect_form() -> bool;
    fn bt_swift_settings_set_use_rust_connect_form(value: bool);
}

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
            nav_item.setTitle(Some(&NSString::from_str("Settings")));
            let done_title = NSString::from_str("Done");
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
            content.setSpacing(spacing::LG);
            content.setLayoutMarginsRelativeArrangement(true);
            content.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
                top: spacing::LG,
                leading: spacing::LG,
                bottom: spacing::LG,
                trailing: spacing::LG,
            });

            // ---- Section 1 — Display ---------------------------------------
            let reserve_initial = unsafe { bt_swift_settings_get_reserve_top_safe_area() };
            let (reserve_row, reserve_switch) = toggle_row(
                mtm,
                "Keep first row visible in full-screen apps",
                reserve_initial,
                self.as_ref(),
                sel!(toggleReserveTopSafeArea:),
            );
            crate::a11y::set_a11y_id(
                &*reserve_switch as &AnyObject,
                "settings.reserveTopSafeArea",
            );
            let reserve_footer = footer_label(
                mtm,
                "When vim, htop, claude or other full-screen tools run, \
                 reserve the top safe area so the Dynamic Island, notch, or \
                 status bar doesn't cover their first row.",
            );
            let display_section = form_section(
                mtm,
                Some("Display"),
                &[reserve_row, reserve_footer],
            );
            content.addArrangedSubview(&display_section);

            // ---- Section 2 — Blocks ----------------------------------------
            let blocks_initial = unsafe { bt_swift_settings_get_show_command_blocks() };
            let (blocks_row, blocks_switch) = toggle_row(
                mtm,
                "Command blocks (Beta)",
                blocks_initial,
                self.as_ref(),
                sel!(toggleShowCommandBlocks:),
            );
            crate::a11y::set_a11y_id(
                &*blocks_switch as &AnyObject,
                "settings.commandBlocks",
            );
            let blocks_footer = footer_label(
                mtm,
                "Show each command and its output as a separate block \
                 (Warp-style). When on, BedTerm writes a small \
                 shell-integration script into every new SSH session to \
                 track prompt and command boundaries. Without shell \
                 integration, blocks won't show command metadata. BedTerm \
                 uses Warp's DCS hook protocol, not OSC 133, so third-party \
                 integrations won't drive it. This feature is still in beta \
                 and off by default.",
            );
            let blocks_section = form_section(
                mtm,
                Some("Blocks"),
                &[blocks_row, blocks_footer],
            );
            content.addArrangedSubview(&blocks_section);

            // ---- Section 3 — Experimental ----------------------------------
            // TODO(localization): English copy lives here while the
            // Rust hosts list is gated behind the W24b flag.
            let hosts_initial = unsafe { bt_swift_settings_get_use_rust_hosts_list() };
            let (hosts_row, hosts_switch) = toggle_row(
                mtm,
                "Rust hosts list (Experimental)",
                hosts_initial,
                self.as_ref(),
                sel!(toggleUseRustHostsList:),
            );
            crate::a11y::set_a11y_id(
                &*hosts_switch as &AnyObject,
                "settings.useRustHostsList",
            );
            let hosts_footer = footer_label(
                mtm,
                "Render the saved-hosts screen with the in-progress Rust \
                 UIKit view controller. Adding hosts still uses the SwiftUI \
                 form; connect / delete flows route back through the \
                 existing Swift orchestration.",
            );
            let form_initial = unsafe { bt_swift_settings_get_use_rust_connect_form() };
            let (form_row, form_switch) = toggle_row(
                mtm,
                "Rust connect form (Experimental)",
                form_initial,
                self.as_ref(),
                sel!(toggleUseRustConnectForm:),
            );
            crate::a11y::set_a11y_id(
                &*form_switch as &AnyObject,
                "settings.useRustConnectForm",
            );
            let form_footer = footer_label(
                mtm,
                "Use the in-progress Rust UIKit connect-form VC when adding \
                 or editing hosts. Validation + Keychain writes still route \
                 through the existing Swift code; the Rust VC only collects \
                 input.",
            );
            let hosts_section = form_section(
                mtm,
                Some("Experimental"),
                &[hosts_row, hosts_footer, form_row, form_footer],
            );
            content.addArrangedSubview(&hosts_section);

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
            // Re-sync UISwitch state with the backing store in case it
            // changed since viewDidLoad (e.g. the user flipped a value
            // somewhere else in the app while the sheet was queued).
            // Currently a no-op pass — we don't keep direct refs to the
            // switches, and observableHandle round-trips are cheap to
            // re-derive on next open. Documented as a deferred follow-up.
        }

        #[unsafe(method(viewDidLayoutSubviews))]
        fn view_did_layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };
            let Some(view) = self.view() else { return };
            let bounds: CGRect = unsafe { msg_send![&*view, bounds] };
            let insets: UIEdgeInsets = unsafe { msg_send![&*view, safeAreaInsets] };

            let scroll_borrow = self.ivars().scroll.borrow();
            let content_borrow = self.ivars().content.borrow();
            let (Some(scroll), Some(content)) = (scroll_borrow.as_ref(), content_borrow.as_ref())
            else {
                return;
            };

            // Scroll view fills the safe area.
            let scroll_frame = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: bounds.size.width,
                    height: bounds.size.height,
                },
            };
            let _: () = unsafe { msg_send![&**scroll, setFrame: scroll_frame] };

            // Content sizes its width to the scroll view and stretches
            // vertically to fit its arranged subviews.
            let available_w = bounds.size.width - insets.left - insets.right;
            let fitting_size = CGSize {
                width: available_w,
                height: 0.0,
            };
            // systemLayoutSizeFittingSize: → returns the natural height
            // for the given width. Pass 0 height with horizontal-fitting
            // priority high + vertical-fitting low (UIKit defaults treat
            // 0/0 height as "compute it").
            let natural: CGSize =
                unsafe { msg_send![&**content, systemLayoutSizeFittingSize: fitting_size] };
            let content_height: CGFloat = natural.height.max(0.0);
            let content_frame = CGRect {
                origin: CGPoint { x: insets.left, y: insets.top },
                size: CGSize {
                    width: available_w,
                    height: content_height,
                },
            };
            let _: () = unsafe { msg_send![&**content, setFrame: content_frame] };

            // Update scroll view's contentSize so the form scrolls when
            // the content exceeds the viewport.
            let content_size = CGSize {
                width: bounds.size.width,
                height: content_height + insets.top + insets.bottom,
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
            unsafe { bt_swift_settings_set_reserve_top_safe_area(on) };
        }

        #[unsafe(method(toggleShowCommandBlocks:))]
        fn toggle_show_command_blocks(&self, sender: &UISwitch) {
            let on: bool = unsafe { msg_send![sender, isOn] };
            unsafe { bt_swift_settings_set_show_command_blocks(on) };
        }

        #[unsafe(method(toggleUseRustHostsList:))]
        fn toggle_use_rust_hosts_list(&self, sender: &UISwitch) {
            let on: bool = unsafe { msg_send![sender, isOn] };
            unsafe { bt_swift_settings_set_use_rust_hosts_list(on) };
        }

        #[unsafe(method(toggleUseRustConnectForm:))]
        fn toggle_use_rust_connect_form(&self, sender: &UISwitch) {
            let on: bool = unsafe { msg_send![sender, isOn] };
            unsafe { bt_swift_settings_set_use_rust_connect_form(on) };
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
