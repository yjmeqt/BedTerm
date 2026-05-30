//! `BtIosHostsListViewController` — Rust saved-hosts list.
//!
//! Owns a `UIScrollView` + vertical `UIStackView` of `make_list_row`
//! cells, one per `HostListEntry`. Row tap calls
//! `bt_ios_hosts_vm_request_connect` on the Rust hosts VM, which fires
//! `RootCoordinator.run_connect` for the full connect→push flow.
//! Left-swipe calls `crate::hosts_store::delete(&id)` directly.
//! The snapshot is read from `crate::hosts_store::list_snapshot_json()`
//! directly. The navbar `+` button fires the `on_add` callback to
//! `RootCoordinator`, which pushes the Rust connect-form VC.
//!
//! All user-facing strings are routed through `bedterm_app::l10n::t(...)`;
//! translations live in `BedTerm/Localizable.xcstrings` and are baked
//! into the binary via `build.rs`.

#![cfg(target_os = "ios")]

use crate::a11y;
use crate::design_system::{
    colors,
    components::{make_list_row_with_accessory, primary_button, ListRowAccessory, ListRowHandle},
    spacing, typography,
};
use crate::hosts::BtIosHostsAddCallback;
use crate::toaster::{BtIosToasterView, ToastAction, ToastKind};
use bedterm_app::geometry::{CGFloat, CGPoint, CGRect, CGSize, UIEdgeInsets};
use bedterm_app::hosts::model::{parse_entries_json, HostListEntry};
use bedterm_app::l10n::t;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Sel};
use objc2::{
    define_class, msg_send, sel, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_foundation::NSString;
use objc2_ui_kit::{
    UIBarButtonItem, UIBarButtonItemStyle, UIButton, UILabel, UILayoutConstraintAxis,
    UINavigationItem, UIScrollView, UIStackView, UIStackViewAlignment, UIStackViewDistribution,
    UISwipeGestureRecognizer, UIView, UIViewController,
};
use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CString};

/// Per-row state pinned for the duration of the rendered snapshot.
struct RowState {
    entry: HostListEntry,
    handle: ListRowHandle,
}

#[derive(Default)]
pub struct Ivars {
    /// Fires on `+` tap. Swift pushes the Rust connect-form VC.
    on_add: Cell<Option<BtIosHostsAddCallback>>,
    /// Opaque host context. SAFETY: never dereffed on Rust side.
    ctx: Cell<*mut c_void>,
    /// Root scroll view + content stack — resized in `viewDidLayoutSubviews`.
    scroll: RefCell<Option<Retained<UIScrollView>>>,
    content: RefCell<Option<Retained<UIStackView>>>,
    /// Empty-state label (hidden when `rows` is non-empty).
    empty_label: RefCell<Option<Retained<UILabel>>>,
    /// Bottom-pinned debug button that triggers a sample toast — handy
    /// for manual checks and as a stable UI-test seam (`hosts.debug.toast`).
    debug_button: RefCell<Option<Retained<UIButton>>>,
    /// Live rows in display order; indices double as button "tag" values
    /// so the tap / swipe selectors can look the entry id back up.
    rows: RefCell<Vec<RowState>>,
}

// SAFETY: only accessed on the main thread (MainThreadOnly).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosHostsListViewController"]
    #[ivars = Ivars]
    pub struct BtIosHostsListViewController;

    impl BtIosHostsListViewController {
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
            if let Some(view) = self.view() {
                view.setBackgroundColor(Some(&colors::shadcn_background()));
                a11y::set_a11y_id(&*view as &AnyObject, "hosts.root");
            }

            // Nav title + `+` button.
            let nav_item: Retained<UINavigationItem> =
                unsafe { msg_send![self, navigationItem] };
            nav_item.setTitle(Some(&NSString::from_str(&t("Hosts"))));
            let add_title = NSString::from_str("+");
            let add_btn: Retained<UIBarButtonItem> = unsafe {
                UIBarButtonItem::initWithTitle_style_target_action(
                    mtm.alloc::<UIBarButtonItem>(),
                    Some(&add_title),
                    UIBarButtonItemStyle::Plain,
                    Some(self.as_ref()),
                    Some(sel!(addTapped)),
                )
            };
            a11y::set_a11y_id(&*add_btn as &AnyObject, "hosts.add");
            nav_item.setRightBarButtonItem(Some(&add_btn));

            // Scroll view + content stack — same layout idiom as Settings VC.
            let scroll: Retained<UIScrollView> = unsafe {
                let alloc = mtm.alloc::<UIScrollView>();
                msg_send![alloc, initWithFrame: CGRect::default()]
            };
            let content = UIStackView::new(mtm);
            content.setAxis(UILayoutConstraintAxis::Vertical);
            content.setAlignment(UIStackViewAlignment::Fill);
            content.setDistribution(UIStackViewDistribution::Fill);
            content.setSpacing(spacing::MD);

            // Empty-state label — hidden until the snapshot lands.
            let empty_label = UILabel::new(mtm);
            empty_label.setText(Some(&NSString::from_str(&t(
                "No hosts yet. Tap + to add one.",
            ))));
            unsafe {
                empty_label.setFont(Some(&typography::body()));
                empty_label.setTextColor(Some(&colors::shadcn_muted_foreground()));
            }
            empty_label.setNumberOfLines(0);
            let _: () = unsafe { msg_send![&*empty_label, setTextAlignment: 1_i64] };
            a11y::set_a11y_id(&*empty_label as &AnyObject, "hosts.empty");

            // Mount scroll → view; content stack pinned in layoutSubviews.
            let content_view: &UIView = unsafe { &*Retained::as_ptr(&content).cast() };
            let _: () = unsafe { msg_send![&*scroll, addSubview: content_view] };
            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addSubview: &*scroll] };
                let label_view = unsafe {
                    &*(&*empty_label as *const UILabel as *const UIView)
                };
                let _: () = unsafe { msg_send![&*view, addSubview: label_view] };
            }
            empty_label.setHidden(true);

            // Debug-only affordance: a bottom-pinned button that triggers a
            // sample toast. Developer string (not localised). Positioned in
            // `viewDidLayoutSubviews` so it survives row reloads (it lives
            // on the VC's view, not inside the cleared content stack).
            let debug_button =
                primary_button(mtm, "ladybug", "Test toast", self.as_ref(), sel!(debugToastTapped));
            a11y::set_a11y_id(&*debug_button as &AnyObject, "hosts.debug.toast");
            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addSubview: &*debug_button] };
            }

            *self.ivars().scroll.borrow_mut() = Some(scroll);
            *self.ivars().content.borrow_mut() = Some(content);
            *self.ivars().empty_label.borrow_mut() = Some(empty_label);
            *self.ivars().debug_button.borrow_mut() = Some(debug_button);
        }

        #[unsafe(method(viewWillAppear:))]
        fn view_will_appear(&self, animated: bool) {
            let _: () = unsafe { msg_send![super(self), viewWillAppear: animated] };
            self.reload_from_swift();
        }

        #[unsafe(method(viewDidLayoutSubviews))]
        fn view_did_layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };
            let Some(view) = self.view() else { return };
            let bounds: CGRect = unsafe { msg_send![&*view, bounds] };
            let insets: UIEdgeInsets = unsafe { msg_send![&*view, safeAreaInsets] };

            let scroll_borrow = self.ivars().scroll.borrow();
            let content_borrow = self.ivars().content.borrow();
            let empty_borrow = self.ivars().empty_label.borrow();
            let (Some(scroll), Some(content), Some(empty)) = (
                scroll_borrow.as_ref(),
                content_borrow.as_ref(),
                empty_borrow.as_ref(),
            ) else {
                return;
            };

            // Scroll view fills the whole bounds. Its
            // `contentInsetAdjustmentBehavior` defaults to `.automatic`, so
            // UIKit subtracts the safe area from the content area for us — we
            // must NOT offset the content stack by `safeAreaInsets.top` again
            // or the content lands twice-pushed below the nav bar.
            let scroll_frame = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: bounds.size.width,
                    height: bounds.size.height,
                },
            };
            let _: () = unsafe { msg_send![&**scroll, setFrame: scroll_frame] };

            // Content stack sized + positioned in scroll-content space.
            let pad: CGFloat = spacing::LG;
            let available_w = bounds.size.width - pad * 2.0;
            let fitting = CGSize {
                width: available_w,
                height: 0.0,
            };
            let natural: CGSize =
                unsafe { msg_send![&**content, systemLayoutSizeFittingSize: fitting] };
            let content_height: CGFloat = natural.height.max(0.0);
            let content_frame = CGRect {
                origin: CGPoint { x: pad, y: pad },
                size: CGSize {
                    width: available_w,
                    height: content_height,
                },
            };
            let _: () = unsafe { msg_send![&**content, setFrame: content_frame] };
            let total_h = content_height + pad * 2.0;
            let content_size = CGSize {
                width: bounds.size.width,
                height: total_h,
            };
            let _: () = unsafe { msg_send![&**scroll, setContentSize: content_size] };

            // Empty label centred in the viewport.
            let empty_frame = CGRect {
                origin: CGPoint {
                    x: insets.left + pad,
                    y: insets.top + pad,
                },
                size: CGSize {
                    width: available_w,
                    height: 120.0,
                },
            };
            let _: () = unsafe {
                msg_send![&**empty as &UILabel, setFrame: empty_frame]
            };

            // Bottom-pinned debug button, centred above the safe-area inset.
            if let Some(debug_button) = self.ivars().debug_button.borrow().as_ref() {
                let btn_w: CGFloat = 180.0;
                let btn_h: CGFloat = 40.0;
                let btn_frame = CGRect {
                    origin: CGPoint {
                        x: (bounds.size.width - btn_w) / 2.0,
                        y: bounds.size.height - insets.bottom - btn_h - spacing::MD,
                    },
                    size: CGSize {
                        width: btn_w,
                        height: btn_h,
                    },
                };
                let _: () = unsafe { msg_send![&**debug_button, setFrame: btn_frame] };
            }
        }

        // ---- Selector handlers -------------------------------------------

        #[unsafe(method(addTapped))]
        fn add_tapped(&self) {
            if let Some(cb) = self.ivars().on_add.get() {
                let ctx = self.ivars().ctx.get();
                unsafe { cb(ctx) };
            }
        }

        /// Debug button: resolve the window-level toaster view and show a
        /// sample success toast. No-op if no toaster is mounted (e.g.
        /// headless tests). Exercises the full Rust toaster path.
        #[unsafe(method(debugToastTapped))]
        fn debug_toast_tapped(&self) {
            let Some(view) = self.view() else { return };
            let window: *mut UIView = unsafe { msg_send![&*view, window] };
            if window.is_null() {
                return;
            }
            let subviews: *mut AnyObject = unsafe { msg_send![window, subviews] };
            if subviews.is_null() {
                return;
            }
            let count: usize = unsafe { msg_send![subviews, count] };
            let toaster_class = BtIosToasterView::class();
            for i in 0..count {
                let obj: *mut AnyObject = unsafe { msg_send![subviews, objectAtIndex: i] };
                let is_toaster: bool =
                    unsafe { msg_send![obj, isKindOfClass: toaster_class] };
                if is_toaster {
                    let toaster = unsafe { &*(obj as *const BtIosToasterView) };
                    let actions = [ToastAction {
                        title: "Retry".to_string(),
                        destructive: false,
                    }];
                    toaster.show(
                        ToastKind::Success,
                        "Test toast",
                        Some("Triggered from the hosts debug button."),
                        false,
                        &actions,
                    );
                    return;
                }
            }
        }

        /// Per-row tap. We can't bind a unique selector per row inside
        /// `define_class!`, so every row's button shares one selector.
        /// `sender.tag` is set to the row index when the row is built.
        #[unsafe(method(rowTapped:))]
        fn row_tapped(&self, sender: &AnyObject) {
            let tag: i64 = unsafe { msg_send![sender, tag] };
            let idx = tag as usize;
            let id_string = {
                let rows = self.ivars().rows.borrow();
                rows.get(idx).map(|r| r.entry.id.clone())
            };
            if let Some(id) = id_string {
                let mut vm = bedterm_app::hosts_vm::VM.lock().unwrap_or_else(|e| e.into_inner());
                let action = vm.request_connect(&id);
                let connect_cb = vm.connect_cb;
                let connect_ctx = vm.connect_ctx;
                drop(vm);
                if let bedterm_app::hosts_vm::Action::Connect(ref cid) = action {
                    if let Some(cb) = connect_cb {
                        if let Ok(cstr) = CString::new(cid.as_str()) {
                            unsafe { cb(connect_ctx.0, cstr.as_ptr()) };
                        }
                    }
                }
            }
        }

        /// Per-row left-swipe. UIKit hands us the recogniser; its
        /// `.view` is the row's outer card UIView. We look up the row
        /// index by pointer-equality against the cached row views.
        #[unsafe(method(rowSwiped:))]
        fn row_swiped(&self, sender: &UISwipeGestureRecognizer) {
            let recog_view: Option<Retained<UIView>> = unsafe {
                let v: *mut UIView = msg_send![sender, view];
                if v.is_null() { None } else { Some(Retained::retain(v).unwrap()) }
            };
            let Some(target_view) = recog_view else { return };
            let id_string = {
                let rows = self.ivars().rows.borrow();
                rows.iter().find_map(|r| {
                    let row_ptr: *const UIView = &*r.handle.row_view;
                    let target_ptr: *const UIView = &*target_view;
                    if row_ptr == target_ptr {
                        Some(r.entry.id.clone())
                    } else {
                        None
                    }
                })
            };
            if let Some(id) = id_string {
                crate::hosts_store::delete(&id);
                self.reload_from_swift();
            }
        }
    }
);

impl BtIosHostsListViewController {
    pub(crate) fn set_callbacks(&self, on_add: Option<BtIosHostsAddCallback>, ctx: *mut c_void) {
        self.ivars().on_add.set(on_add);
        self.ivars().ctx.set(ctx);
    }

    // TODO(Phase 4): present_alert via objc2 UIAlertController.
    // Blocked on msg_send! comma-syntax migration and block2 handler ABI
    // verification on device. The Swift HostsConnectController still owns
    // alert presentation; this method will replace it once tested.

    /// Pull a fresh snapshot from the Rust-owned hosts store and re-render
    /// the rows. Idempotent.
    pub(crate) fn reload_from_swift(&self) {
        let json = crate::hosts_store::list_snapshot_json();
        let entries = parse_entries_json(&json);
        self.render_entries(entries);
    }

    fn render_entries(&self, entries: Vec<HostListEntry>) {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let content_borrow = self.ivars().content.borrow();
        let empty_borrow = self.ivars().empty_label.borrow();
        let (Some(content), Some(empty)) = (content_borrow.as_ref(), empty_borrow.as_ref()) else {
            return;
        };

        // Tear down old rows.
        let arranged: Retained<AnyObject> = unsafe { msg_send![&**content, arrangedSubviews] };
        let count: usize = unsafe { msg_send![&*arranged, count] };
        for _ in 0..count {
            // Always remove index 0 since the array compacts.
            let v: *mut UIView = unsafe { msg_send![&*arranged, objectAtIndex: 0_usize] };
            if !v.is_null() {
                unsafe {
                    let _: () = msg_send![&**content, removeArrangedSubview: v];
                    let _: () = msg_send![v, removeFromSuperview];
                }
            }
        }
        self.ivars().rows.borrow_mut().clear();

        let is_empty = entries.is_empty();
        empty.setHidden(!is_empty);

        for (idx, entry) in entries.into_iter().enumerate() {
            let title = entry.primary_label();
            let subtitle = entry.subtitle();
            // Trailing accessory: key.fill when the entry authenticates
            // with a private key, lock.fill for password-based hosts.
            let accessory = ListRowAccessory::Badge {
                system_name: if entry.auth_is_key {
                    "key.fill"
                } else {
                    "lock.fill"
                },
            };
            let (row_view, handle) =
                make_list_row_with_accessory(mtm, &title, subtitle.as_deref(), accessory);

            // Tag the button with the row index so `rowTapped:` can look
            // the entry id back up.
            unsafe {
                let _: () = msg_send![&*handle.button, setTag: idx as i64];
            }
            handle.set_on_tap(self.as_ref(), sel!(rowTapped:));
            handle.set_on_delete(mtm, self.as_ref(), sel!(rowSwiped:));

            // W24d: tag the inner UIButton (not the card UIView) so
            // XCUITest's `app.buttons["hosts.row.<id>"]` query resolves
            // to exactly one element.
            let row_id = format!("hosts.row.{}", entry.id);
            a11y::set_a11y_id(&*handle.button as &AnyObject, &row_id);

            content.addArrangedSubview(&row_view);

            self.ivars()
                .rows
                .borrow_mut()
                .push(RowState { entry, handle });
        }

        // Force a re-layout so the freshly-added rows pick up frames.
        if let Some(view) = self.view() {
            unsafe {
                let _: () = msg_send![&*view, setNeedsLayout];
            }
        }
    }
}

pub(crate) unsafe fn create_hosts_list_vc(
    on_add: Option<BtIosHostsAddCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let vc: Retained<BtIosHostsListViewController> =
        unsafe { msg_send![mtm.alloc::<BtIosHostsListViewController>(), init] };
    vc.set_callbacks(on_add, ctx);
    Retained::into_raw(vc) as *mut c_void
}

pub(crate) unsafe fn release_hosts_list_vc(vc_ptr: *mut c_void) {
    if vc_ptr.is_null() {
        return;
    }
    let _ = unsafe { Retained::from_raw(vc_ptr as *mut BtIosHostsListViewController) };
}

// Silence unused-import warnings on the `Sel` re-export — the type is
// reached for via `sel!(...)` macro expansion which doesn't show up to
// the linter.
#[allow(dead_code)]
fn _sel_ref_keeper(s: Sel) -> Sel {
    s
}
