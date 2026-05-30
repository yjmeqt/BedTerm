//! `BtIosConnectFormViewController` — Rust port of `ConnectionFormScreen`.
//!
//! Owns a `UIScrollView` + vertical `UIStackView` of three form
//! sections: Identity (label), Connection (host / port / username), and
//! Authentication (segmented control + password, or "Coming Soon" for key).
//! Key auth is not yet implemented; selecting it shows a placeholder.

#![cfg(target_os = "ios")]

use crate::a11y;
use crate::connect_form::{BtIosConnectFormCancelCallback, BtIosConnectFormDoneCallback};
use crate::design_system::{
    colors,
    components::{
        form_card, make_secure_text_field, make_segmented_control, make_text_field,
        TextFieldConfig, TextFieldHandle,
    },
    spacing, typography,
};
use bedterm_app::connect_form_vm;
use bedterm_app::geometry::{CGFloat, CGPoint, CGRect, CGSize, UIEdgeInsets};
use bedterm_app::l10n::t;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::NSString;
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIBarButtonItem, UIBarButtonItemStyle, UILabel,
    UILayoutConstraintAxis, UINavigationItem, UIScrollView, UISegmentedControl, UIStackView,
    UIStackViewAlignment, UIStackViewDistribution, UIView, UIViewController,
};
use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CString};

/// Internal layout pin — the auth section's secret rows toggle visibility
/// when the segmented control flips. We keep retained refs to both
/// stacks and to all field handles so the selectors can read state out.
#[derive(Default)]
pub struct Ivars {
    on_done: Cell<Option<BtIosConnectFormDoneCallback>>,
    on_cancel: Cell<Option<BtIosConnectFormCancelCallback>>,
    ctx: Cell<*mut c_void>,

    scroll: RefCell<Option<Retained<UIScrollView>>>,
    content: RefCell<Option<Retained<UIStackView>>>,

    label_field: RefCell<Option<TextFieldHandle>>,
    host_field: RefCell<Option<TextFieldHandle>>,
    port_field: RefCell<Option<TextFieldHandle>>,
    username_field: RefCell<Option<TextFieldHandle>>,
    password_field: RefCell<Option<TextFieldHandle>>,

    segmented: RefCell<Option<Retained<UISegmentedControl>>>,
    password_row: RefCell<Option<Retained<UIView>>>,
    key_coming_soon_row: RefCell<Option<Retained<UIView>>>,
    error_label: RefCell<Option<Retained<UIView>>>,

    editing_id: RefCell<Option<String>>,
    connect_on_save: Cell<bool>,
    _password_touched: Cell<bool>,
    password_set: Cell<bool>,
}

// SAFETY: only accessed on the main thread (MainThreadOnly).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosConnectFormViewController"]
    #[ivars = Ivars]
    pub struct BtIosConnectFormViewController;

    impl BtIosConnectFormViewController {
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
                a11y::set_a11y_id(&*view as &AnyObject, "connection.rust.root");
            }

            // Bar buttons.
            let nav_item: Retained<UINavigationItem> =
                unsafe { msg_send![self, navigationItem] };
            let is_edit = self.ivars().editing_id.borrow().is_some();
            let title = if is_edit { t("Edit Host") } else { t("New Host") };
            nav_item.setTitle(Some(&NSString::from_str(&title)));
            // Cancel replaces the inherited back chevron — the parent
            // navigation controller would otherwise push us with the
            // default back button visible alongside our Cancel bar item.
            unsafe {
                let _: () = msg_send![&*nav_item, setHidesBackButton: true];
            }

            let cancel_title = NSString::from_str(&t("Cancel"));
            let cancel_btn: Retained<UIBarButtonItem> = unsafe {
                UIBarButtonItem::initWithTitle_style_target_action(
                    mtm.alloc::<UIBarButtonItem>(),
                    Some(&cancel_title),
                    UIBarButtonItemStyle::Plain,
                    Some(self.as_ref()),
                    Some(sel!(cancelTapped)),
                )
            };
            a11y::set_a11y_id(&*cancel_btn as &AnyObject, "connection.cancel");
            nav_item.setLeftBarButtonItem(Some(&cancel_btn));

            let save_title_str = if self.ivars().connect_on_save.get() {
                t("Save & Connect")
            } else {
                t("Save")
            };
            let save_title = NSString::from_str(&save_title_str);
            let save_btn: Retained<UIBarButtonItem> = unsafe {
                UIBarButtonItem::initWithTitle_style_target_action(
                    mtm.alloc::<UIBarButtonItem>(),
                    Some(&save_title),
                    UIBarButtonItemStyle::Plain,
                    Some(self.as_ref()),
                    Some(sel!(saveTapped)),
                )
            };
            a11y::set_a11y_id(&*save_btn as &AnyObject, "connection.save");
            nav_item.setRightBarButtonItem(Some(&save_btn));

            // ---- Scroll + content stack -----------------------------------
            let scroll: Retained<UIScrollView> = unsafe {
                let alloc = mtm.alloc::<UIScrollView>();
                msg_send![alloc, initWithFrame: CGRect::default()]
            };
            let content = UIStackView::new(mtm);
            content.setAxis(UILayoutConstraintAxis::Vertical);
            content.setAlignment(UIStackViewAlignment::Fill);
            content.setDistribution(UIStackViewDistribution::Fill);
            // Cards separated by 12 pt vertically, matching the SwiftUI
            // `ConnectionFormScreen` outer `VStack(spacing: 12)`. The
            // surrounding 16 pt screen padding is applied via this
            // stack's directional layout margins below.
            content.setSpacing(spacing::MD);
            content.setLayoutMarginsRelativeArrangement(true);
            content.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
                top: spacing::LG,
                leading: spacing::LG,
                bottom: spacing::LG,
                trailing: spacing::LG,
            });

            // ---- Connection card -----------------------------------------
            // Two-card layout matching the SwiftUI `ConnectionFormScreen`:
            // Card 1 ("Connection") groups Label + Host + Port + Username;
            // Card 2 ("Authentication") groups the segmented control plus
            // either the password row or the key-picker + passphrase rows.
            // TODO(localization): the SwiftUI source uses `String(localized:)`
            // for these card titles + descriptions; mirror once the Rust
            // layer has a string-catalogue bridge.
            let (label_row, label_handle) = make_text_field(
                mtm,
                &t("Label"),
                &t("Personal Mac"),
                TextFieldConfig {
                    secure: false,
                    keyboard_type: 0,
                    autocapitalization: 0,
                    autocorrection: 1,
                },
            );
            a11y::set_a11y_id(&*label_handle.field as &AnyObject, "connection.label");

            let (host_row, host_handle) = make_text_field(
                mtm,
                &t("Host"),
                "10.0.0.5",
                TextFieldConfig {
                    secure: false,
                    keyboard_type: 0,
                    autocapitalization: 0,
                    autocorrection: 1,
                },
            );
            a11y::set_a11y_id(&*host_handle.field as &AnyObject, "connection.host");
            let (port_row, port_handle) = make_text_field(
                mtm,
                &t("Port"),
                "22",
                TextFieldConfig {
                    secure: false,
                    keyboard_type: 4, // UIKeyboardTypeNumberPad
                    autocapitalization: 0,
                    autocorrection: 1,
                },
            );
            a11y::set_a11y_id(&*port_handle.field as &AnyObject, "connection.port");
            port_handle.set_text("22");
            let (username_row, username_handle) = make_text_field(
                mtm,
                &t("Username"),
                "root",
                TextFieldConfig {
                    secure: false,
                    keyboard_type: 0,
                    autocapitalization: 0,
                    autocorrection: 1,
                },
            );
            a11y::set_a11y_id(&*username_handle.field as &AnyObject, "connection.username");

            let connection_section = form_card(
                mtm,
                &t("Connection"),
                Some(&t("Where to reach the server. Host can be an IP or hostname.")),
                &[label_row, host_row, port_row, username_row],
            );
            content.addArrangedSubview(&connection_section);
            *self.ivars().label_field.borrow_mut() = Some(label_handle);
            *self.ivars().host_field.borrow_mut() = Some(host_handle);
            *self.ivars().port_field.borrow_mut() = Some(port_handle);
            *self.ivars().username_field.borrow_mut() = Some(username_handle);

            // ---- Authentication card -------------------------------------
            let seg_password = t("Password");
            let seg_key = t("Key");
            let segmented = make_segmented_control(
                mtm,
                &[seg_password.as_str(), seg_key.as_str()],
                0,
                self.as_ref(),
                sel!(authModeChanged:),
            );
            a11y::set_a11y_id(&*segmented as &AnyObject, "connection.authMode");
            let seg_view: Retained<UIView> = unsafe {
                Retained::cast_unchecked::<UIView>(segmented.clone())
            };

            let (password_row, password_handle) =
                make_secure_text_field(mtm, &t("Password"), &t("Password"));
            a11y::set_a11y_id(
                &*password_handle.field as &AnyObject,
                "connection.password",
            );

            // Key "Coming Soon" placeholder label.
            let key_coming_soon = UILabel::new(mtm);
            key_coming_soon.setText(Some(&NSString::from_str(&t("Key authentication coming soon."))));
            key_coming_soon.setNumberOfLines(0);
            key_coming_soon.setTextAlignment(objc2_ui_kit::NSTextAlignment(1)); // NSTextAlignmentCenter
            unsafe {
                key_coming_soon.setFont(Some(&typography::caption()));
                key_coming_soon.setTextColor(Some(&colors::shadcn_muted_foreground()));
            }
            a11y::set_a11y_id(&*key_coming_soon as &AnyObject, "connection.keyComingSoon");
            let key_coming_soon_row: Retained<UIView> =
                unsafe { Retained::cast_unchecked::<UIView>(key_coming_soon) };
            key_coming_soon_row.setHidden(true);

            let auth_section = form_card(
                mtm,
                &t("Authentication"),
                Some(&t("Choose how to prove identity to the server.")),
                &[
                    seg_view,
                    password_row.clone(),
                    key_coming_soon_row.clone(),
                ],
            );
            content.addArrangedSubview(&auth_section);

            // Error label.
            let error_view = make_error_label(mtm, "");
            error_view.setHidden(true);
            content.addArrangedSubview(&error_view);

            *self.ivars().segmented.borrow_mut() = Some(segmented);
            *self.ivars().password_row.borrow_mut() = Some(password_row);
            *self.ivars().key_coming_soon_row.borrow_mut() = Some(key_coming_soon_row);
            *self.ivars().password_field.borrow_mut() = Some(password_handle);
            *self.ivars().error_label.borrow_mut() = Some(error_view);

            // Mount.
            let content_view: &UIView = unsafe { &*Retained::as_ptr(&content).cast() };
            let _: () = unsafe { msg_send![&*scroll, addSubview: content_view] };
            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addSubview: &*scroll] };
            }
            *self.ivars().scroll.borrow_mut() = Some(scroll);
            *self.ivars().content.borrow_mut() = Some(content);

            // Apply prefill (edit mode) AFTER all rows are created.
            self.apply_prefill();
        }

        #[unsafe(method(viewDidLayoutSubviews))]
        fn view_did_layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };
            let Some(view) = self.view() else { return };
            let bounds: CGRect = unsafe { msg_send![&*view, bounds] };
            let insets: UIEdgeInsets = unsafe { msg_send![&*view, safeAreaInsets] };

            let scroll_borrow = self.ivars().scroll.borrow();
            let content_borrow = self.ivars().content.borrow();
            let (Some(scroll), Some(content)) =
                (scroll_borrow.as_ref(), content_borrow.as_ref())
            else { return };

            // Scroll fills the bounds; UIScrollView's
            // `contentInsetAdjustmentBehavior = .automatic` handles the safe
            // area (nav bar + status bar + home indicator) for us. Don't
            // offset the content stack by `safeAreaInsets` again — that
            // produced the visible 2× safe-area gap (scroll bounds.origin.y
            // = -116 plus content frame.y = +116, visual y = 232).
            let scroll_frame = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: bounds.size.width,
                    height: bounds.size.height,
                },
            };
            let _: () = unsafe { msg_send![&**scroll, setFrame: scroll_frame] };
            let _ = insets;

            let available_w = bounds.size.width;
            let fitting = CGSize {
                width: available_w,
                height: 0.0,
            };
            let natural: CGSize =
                unsafe { msg_send![&**content, systemLayoutSizeFittingSize: fitting] };
            let h: CGFloat = natural.height.max(0.0);
            let content_frame = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize { width: available_w, height: h },
            };
            let _: () = unsafe { msg_send![&**content, setFrame: content_frame] };
            let _: () = unsafe {
                msg_send![&**scroll, setContentSize: CGSize {
                    width: bounds.size.width,
                    height: h,
                }]
            };
        }

        // ---- Selectors ---------------------------------------------------

        #[unsafe(method(cancelTapped))]
        fn cancel_tapped(&self) {
            if let Some(cb) = self.ivars().on_cancel.get() {
                unsafe { cb(self.ivars().ctx.get()) };
            }
        }

        #[unsafe(method(saveTapped))]
        fn save_tapped(&self) {
            self.try_save();
        }

        #[unsafe(method(authModeChanged:))]
        fn auth_mode_changed(&self, sender: &UISegmentedControl) {
            let idx: i64 = unsafe { msg_send![sender, selectedSegmentIndex] };
            self.update_auth_rows_visibility(idx);
        }
    }
);

impl BtIosConnectFormViewController {
    pub(crate) fn set_callbacks(
        &self,
        on_done: Option<BtIosConnectFormDoneCallback>,
        on_cancel: Option<BtIosConnectFormCancelCallback>,
        ctx: *mut c_void,
    ) {
        self.ivars().on_done.set(on_done);
        self.ivars().on_cancel.set(on_cancel);
        self.ivars().ctx.set(ctx);
    }

    pub(crate) fn set_editing_id(&self, id: Option<String>) {
        *self.ivars().editing_id.borrow_mut() = id;
    }

    pub(crate) fn set_connect_on_save(&self, value: bool) {
        self.ivars().connect_on_save.set(value);
    }

    fn apply_prefill(&self) {
        let editing = self.ivars().editing_id.borrow().clone();
        let Some(id) = editing else {
            // Add mode — reset the VM to a clean slate.
            connect_form_vm::VM.lock().unwrap().reset_to_add();
            return;
        };

        // Load the Swift-encoded SavedHost JSON blob from the Keychain
        // via the Rust hosts_store.
        let Some(json_str) = crate::hosts_store::load_json(&id) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) else {
            return;
        };
        let Some(obj) = value.as_object() else { return };
        let Some(credential) = obj.get("credential").and_then(|v| v.as_object()) else {
            return;
        };

        let label = obj
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let host = credential
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let port = credential
            .get("port")
            .and_then(|v| v.as_i64())
            .unwrap_or(22) as u16;
        let username = credential
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let auth_obj = credential.get("auth").and_then(|v| v.as_object());
        let auth_is_key = auth_obj
            .map(|o| o.contains_key("privateKey"))
            .unwrap_or(false);

        let existing_password = auth_obj
            .and_then(|o| o.get("password"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Push all extracted fields to the VM.
        let (vm_label, vm_host, vm_port_text, vm_username, vm_is_using_key, pw_set) = {
            let mut vm = connect_form_vm::VM.lock().unwrap();
            vm.prefill_edit(
                id,
                label,
                host,
                port,
                username,
                auth_is_key,
                existing_password,
            );
            (
                vm.label.clone(),
                vm.host.clone(),
                vm.port.clone(),
                vm.username.clone(),
                vm.is_using_key,
                vm.has_password(),
            )
        };

        // Populate UI fields from the VM (the authority).
        if let Some(h) = self.ivars().label_field.borrow().as_ref() {
            h.set_text(&vm_label);
        }
        if let Some(h) = self.ivars().host_field.borrow().as_ref() {
            h.set_text(&vm_host);
        }
        if let Some(h) = self.ivars().port_field.borrow().as_ref() {
            h.set_text(&vm_port_text);
        }
        if let Some(h) = self.ivars().username_field.borrow().as_ref() {
            h.set_text(&vm_username);
        }

        let seg_idx: i64 = if vm_is_using_key { 1 } else { 0 };
        if let Some(seg) = self.ivars().segmented.borrow().as_ref() {
            let _: () = unsafe { msg_send![&**seg, setSelectedSegmentIndex: seg_idx] };
        }
        self.update_auth_rows_visibility(seg_idx);

        self.ivars().password_set.set(pw_set);
    }

    fn update_auth_rows_visibility(&self, mode: i64) {
        let password_hidden = mode != 0;
        let key_coming_soon_hidden = mode != 1;
        if let Some(v) = self.ivars().password_row.borrow().as_ref() {
            v.setHidden(password_hidden);
        }
        if let Some(v) = self.ivars().key_coming_soon_row.borrow().as_ref() {
            v.setHidden(key_coming_soon_hidden);
        }
        if let Some(view) = self.view() {
            unsafe {
                let _: () = msg_send![&*view, setNeedsLayout];
            }
        }
    }

    fn try_save(&self) {
        // Read all UI field values.
        let label = self
            .ivars()
            .label_field
            .borrow()
            .as_ref()
            .map(|h| h.text())
            .unwrap_or_default();
        let host = self
            .ivars()
            .host_field
            .borrow()
            .as_ref()
            .map(|h| h.text())
            .unwrap_or_default();
        let port_text = self
            .ivars()
            .port_field
            .borrow()
            .as_ref()
            .map(|h| h.text())
            .unwrap_or_default();
        let username = self
            .ivars()
            .username_field
            .borrow()
            .as_ref()
            .map(|h| h.text())
            .unwrap_or_default();
        let password = self
            .ivars()
            .password_field
            .borrow()
            .as_ref()
            .map(|h| h.text())
            .unwrap_or_default();
        let mode_idx: i64 = self
            .ivars()
            .segmented
            .borrow()
            .as_ref()
            .map(|s| unsafe { msg_send![&**s, selectedSegmentIndex] })
            .unwrap_or(0);
        let is_using_key = mode_idx == 1;

        // Push values to the VM then validate + save.
        let mut vm = connect_form_vm::VM.lock().unwrap();
        vm.set_label(label);
        vm.set_host(host);
        vm.set_port_text(port_text);
        vm.set_username(username);
        vm.set_using_key(is_using_key);
        if !password.is_empty() {
            vm.set_password(password);
        }

        // Validate through the VM.
        if let Some(err) = vm.validate() {
            self.show_error(&err);
            return; // vm MutexGuard drops here
        }

        // Save through the VM — resolves secrets, generates/retains UUID.
        match vm.try_save() {
            Ok(outcome) => {
                let id = outcome.id.clone();
                let host = bedterm_app::hosts::model::SavedHost::from(outcome);
                drop(vm); // Release VM lock before persistence.

                // Persist directly through the Rust Keychain store.
                crate::hosts_store::save_host(&host);

                // Dispatch on_done.
                let id_c = CString::new(id).unwrap_or_default();
                let connect_now = self.ivars().connect_on_save.get();
                if let Some(cb) = self.ivars().on_done.get() {
                    let ctx = self.ivars().ctx.get();
                    unsafe { cb(ctx, id_c.as_ptr(), connect_now) };
                }
            }
            Err(_) => {
                let msg = vm
                    .error_message
                    .clone()
                    .unwrap_or_else(|| t("Could not save host."));
                self.show_error(&msg);
                // vm MutexGuard drops here
            }
        }
    }

    fn show_error(&self, message: &str) {
        if let Some(view) = self.ivars().error_label.borrow().as_ref() {
            set_error_message(view, message);
            view.setHidden(false);
        }
    }
}

fn make_error_label(mtm: MainThreadMarker, text: &str) -> Retained<UIView> {
    use objc2_ui_kit::UILabel;
    let label = UILabel::new(mtm);
    label.setText(Some(&NSString::from_str(text)));
    label.setNumberOfLines(0);
    unsafe {
        label.setFont(Some(&typography::caption()));
        label.setTextColor(Some(&colors::shadcn_destructive()));
    }
    a11y::set_a11y_id(&*label as &AnyObject, "connection.error");
    let stack = UIStackView::new(mtm);
    stack.setAxis(UILayoutConstraintAxis::Vertical);
    stack.setAlignment(UIStackViewAlignment::Fill);
    stack.setDistribution(UIStackViewDistribution::Fill);
    stack.setLayoutMarginsRelativeArrangement(true);
    stack.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
        top: spacing::SM,
        leading: spacing::LG,
        bottom: spacing::SM,
        trailing: spacing::LG,
    });
    stack.addArrangedSubview(&label);
    unsafe { Retained::cast_unchecked::<UIView>(stack) }
}

fn set_error_message(view: &UIView, message: &str) {
    use objc2::ClassType;
    use objc2_ui_kit::UILabel;
    // Recursive view-tree walk — the error label is wrapped inside a
    // UIStackView with directional margins, and may pick up extra
    // intermediate wrappers when swift-format / the swiftlint rules
    // reshape this code path. Hand-rolled stack avoids allocator churn.
    let mut stack: Vec<*const AnyObject> = vec![view as *const UIView as *const AnyObject];
    while let Some(node_ptr) = stack.pop() {
        if node_ptr.is_null() {
            continue;
        }
        let subviews: Retained<AnyObject> = unsafe { msg_send![node_ptr, subviews] };
        let count: usize = unsafe { msg_send![&*subviews, count] };
        for i in 0..count {
            let child: *mut AnyObject = unsafe { msg_send![&*subviews, objectAtIndex: i] };
            if child.is_null() {
                continue;
            }
            let is_label: bool = unsafe { msg_send![child, isKindOfClass: UILabel::class()] };
            if is_label {
                unsafe {
                    let ns = NSString::from_str(message);
                    let _: () = msg_send![child, setText: &*ns];
                }
                return;
            }
            stack.push(child as *const AnyObject);
        }
    }
}

pub(crate) unsafe fn create_connect_form_vc(
    editing_id: Option<String>,
    connect_on_save: bool,
    on_done: Option<BtIosConnectFormDoneCallback>,
    on_cancel: Option<BtIosConnectFormCancelCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let vc: Retained<BtIosConnectFormViewController> =
        unsafe { msg_send![mtm.alloc::<BtIosConnectFormViewController>(), init] };
    vc.set_editing_id(editing_id);
    vc.set_connect_on_save(connect_on_save);
    vc.set_callbacks(on_done, on_cancel, ctx);
    Retained::into_raw(vc) as *mut c_void
}

pub(crate) unsafe fn release_connect_form_vc(vc_ptr: *mut c_void) {
    if vc_ptr.is_null() {
        return;
    }
    let _ = unsafe { Retained::from_raw(vc_ptr as *mut BtIosConnectFormViewController) };
}
