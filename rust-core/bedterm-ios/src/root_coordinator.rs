//! `RootCoordinator` — Rust replacement for Swift's `RootCoordinator.swift`.
//!
//! Owns the app window, nav stack, toaster overlay, and connect
//! orchestration. Entry point: `bt_ios_start_root_coordinator(window_ptr)`.

use objc2::rc::Retained;
use objc2::{msg_send, MainThreadMarker};
use objc2_foundation::NSString;
use objc2_ui_kit::{
    UIBarButtonItem, UIBarButtonItemStyle, UILabel, UINavigationController, UIView,
    UIViewController, UIWindow,
};
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CString};

use crate::connect_form::connect_form_vc::create_connect_form_vc;
use crate::host_key_mismatch_vc;
use crate::hosts::hosts_vc::create_hosts_list_vc;
use crate::hosts_store;
use crate::terminal_session::BtTerminalSessionHandle;
use crate::toaster::create_toaster_view;
use crate::vc;

use bedterm_app::connect_form_vm;
use bedterm_app::hosts_vm;

static mut COORDINATOR: Option<RootCoordinator> = None;

pub struct RootCoordinator {
    window: Retained<UIWindow>,
    nav: RefCell<Option<Retained<UINavigationController>>>,
    toaster_view: RefCell<Option<Retained<UIView>>>,
    hosts_vc: RefCell<Option<Retained<UIViewController>>>,
    session_handle: RefCell<Option<*mut BtTerminalSessionHandle>>,
    mtm: MainThreadMarker,
}

// SAFETY: RootCoordinator is only accessed from the main thread.
unsafe impl Send for RootCoordinator {}
unsafe impl Sync for RootCoordinator {}

impl RootCoordinator {
    pub fn new(mtm: MainThreadMarker, window: Retained<UIWindow>) -> Self {
        Self {
            window,
            nav: RefCell::new(None),
            toaster_view: RefCell::new(None),
            hosts_vc: RefCell::new(None),
            session_handle: RefCell::new(None),
            mtm,
        }
    }

    pub fn start(&self) {
        self.install_hosts_root();
        self.window.makeKeyAndVisible();
        self.install_toaster_overlay();
    }

    // -- Hosts root ------------------------------------------------------

    fn install_hosts_root(&self) {
        self.install_vm_callbacks();
        hosts_vm_load_from_store();

        let ctx = self as *const Self as *mut c_void;
        let raw = unsafe { create_hosts_list_vc(Some(add_trampoline), ctx) };
        let vc = unsafe { Retained::retain(raw as *mut UIViewController).unwrap() };
        *self.hosts_vc.borrow_mut() = Some(vc);

        let hosts_vc_ref = self.hosts_vc.borrow();
        let root: &UIViewController = hosts_vc_ref.as_ref().unwrap();
        let nav = UINavigationController::initWithRootViewController(
            self.mtm.alloc::<UINavigationController>(),
            root,
        );
        *self.nav.borrow_mut() = Some(nav);

        self.window
            .setRootViewController(Some(self.nav.borrow().as_ref().unwrap()));
        self.bring_toaster_to_front();
        self.refresh_hosts_list();
    }

    // -- VM callbacks ----------------------------------------------------

    fn install_vm_callbacks(&self) {
        let ctx = self as *const Self as *mut c_void;
        hosts_vm_lock().set_callbacks(
            Some(connect_trampoline),
            hosts_vm::CallbackCtx(ctx),
            Some(disconnect_trampoline),
            hosts_vm::CallbackCtx(ctx),
            None,
            hosts_vm::CallbackCtx(std::ptr::null_mut()),
        );
        connect_form_vm_lock().set_callbacks(Some(form_save_trampoline), ctx);
    }

    // -- Connect flow ----------------------------------------------------

    fn run_connect(&self, id_str: &str) {
        let Some(saved) = hosts_store::load_host(id_str) else {
            let id_c = CString::new(id_str).unwrap_or_default();
            hosts_vm_lock().connect_completed_error(
                unsafe { std::ffi::CStr::from_ptr(id_c.as_ptr()) }
                    .to_str()
                    .unwrap_or(""),
            );
            return;
        };

        let h = unsafe {
            crate::terminal_session::bt_terminal_session_create(None, std::ptr::null_mut())
        };
        if h.is_null() {
            let id_c = CString::new(id_str).unwrap_or_default();
            hosts_vm_lock().connect_completed_error(
                unsafe { std::ffi::CStr::from_ptr(id_c.as_ptr()) }
                    .to_str()
                    .unwrap_or(""),
            );
            return;
        }

        let hname = saved.credential.host.clone();
        let hc = CString::new(&*hname).unwrap_or_default();
        let uc = CString::new(&*saved.credential.username).unwrap_or_default();
        let p = saved.credential.port;

        let cred = credential_for_terminal(&saved.credential.auth);
        let cred_json = serde_json::to_string(&cred).unwrap_or_default();
        let ac = CString::new(cred_json).unwrap_or_default();

        unsafe {
            crate::terminal_session::bt_terminal_session_connect(
                h,
                hc.as_ptr(),
                p,
                uc.as_ptr(),
                ac.as_ptr(),
                std::ptr::null(),
                80,
                24,
                5000,
                noop_completion,
                std::ptr::null_mut(),
            );
        }

        *self.session_handle.borrow_mut() = Some(h);
        let id_c = CString::new(id_str).unwrap_or_default();
        hosts_vm_lock().connect_completed_session(
            unsafe { std::ffi::CStr::from_ptr(id_c.as_ptr()) }
                .to_str()
                .unwrap_or(""),
        );
        self.push_terminal(h, &hname, p);
    }

    fn push_terminal(&self, h: *mut BtTerminalSessionHandle, host: &str, _port: u16) {
        let ctx = self as *const Self as *mut c_void;
        let raw = unsafe { vc::create_vc(Some(back_trampoline), ctx) };
        if raw.is_null() {
            return;
        }
        let vc: Retained<UIViewController> =
            unsafe { Retained::retain(raw as *mut UIViewController).unwrap() };
        vc.loadViewIfNeeded();
        let mv = unsafe { bt_ios_vc_metal_view(raw) };
        if !mv.is_null() {
            unsafe {
                crate::terminal_session::bt_terminal_session_attach_metal_view(h, mv);
            }
        }

        let title = UILabel::new(self.mtm);
        title.setText(Some(&NSString::from_str(host)));
        vc.navigationItem().setTitleView(Some(&title));

        let back = unsafe {
            UIBarButtonItem::initWithTitle_style_target_action(
                self.mtm.alloc::<UIBarButtonItem>(),
                Some(&NSString::from_str("\u{2039}")),
                UIBarButtonItemStyle::Plain,
                None,
                None,
            )
        };
        vc.navigationItem().setLeftBarButtonItem(Some(&back));
        vc.navigationItem().setHidesBackButton(true);

        if let Some(ref nav) = *self.nav.borrow() {
            let _: () = unsafe { msg_send![&**nav, pushViewController: &*vc, animated: true] };
        }
    }

    fn disconnect_session(&self) {
        if let Some(h) = self.session_handle.borrow_mut().take() {
            unsafe {
                crate::terminal_session::bt_terminal_session_close(h);
            }
        }
    }

    fn refresh_hosts_list(&self) {
        hosts_vm_load_from_store();
        if let Some(ref vc) = *self.hosts_vc.borrow() {
            unsafe {
                let _: () = msg_send![vc, beginAppearanceTransition: true, animated: false];
                let _: () = msg_send![vc, endAppearanceTransition];
            }
        }
    }

    // -- Nav presentations -----------------------------------------------

    #[allow(dead_code)]
    fn present_settings(&self) {
        let ctx = self as *const Self as *mut c_void;
        let raw =
            unsafe { crate::settings_vc::create_settings_vc(Some(settings_done_trampoline), ctx) };
        if raw.is_null() {
            return;
        }
        let vc: Retained<UIViewController> =
            unsafe { Retained::retain(raw as *mut UIViewController).unwrap() };
        let modal = UINavigationController::initWithRootViewController(
            self.mtm.alloc::<UINavigationController>(),
            &vc,
        );
        if let Some(ref nav) = *self.nav.borrow() {
            unsafe {
                let _: () = msg_send![&**nav, presentViewController: &*modal, animated: true, completion: std::ptr::null::<c_void>()];
            }
        }
    }

    fn present_host_form(&self) {
        let ctx = self as *const Self as *mut c_void;
        let raw = unsafe {
            create_connect_form_vc(
                None,  // add mode
                false, // connect_on_save
                Some(form_done_trampoline),
                Some(form_cancel_trampoline),
                ctx,
            )
        };
        if raw.is_null() {
            return;
        }
        let vc: Retained<UIViewController> =
            unsafe { Retained::retain(raw as *mut UIViewController).unwrap() };
        if let Some(ref nav) = *self.nav.borrow() {
            let _: () = unsafe { msg_send![&**nav, pushViewController: &*vc, animated: true] };
        }
    }

    #[allow(dead_code)]
    fn present_mismatch(&self) {
        let ctx = self as *const Self as *mut c_void;
        let raw = unsafe {
            host_key_mismatch_vc::create_mismatch_vc(
                Some(mismatch_trust_trampoline),
                Some(mismatch_reject_trampoline),
                ctx,
            )
        };
        if raw.is_null() {
            return;
        }
        let vc: Retained<UIViewController> =
            unsafe { Retained::retain(raw as *mut UIViewController).unwrap() };
        if let Some(ref nav) = *self.nav.borrow() {
            unsafe {
                let _: () = msg_send![&**nav, presentViewController: &*vc, animated: true, completion: std::ptr::null::<c_void>()];
            }
        }
    }

    // -- Toaster ---------------------------------------------------------

    fn install_toaster_overlay(&self) {
        let view = create_toaster_view();
        let v: Retained<UIView> = Retained::into_super(view);
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        let _: () = unsafe { msg_send![&*self.window, addSubview: &*v] };
        self.window.bringSubviewToFront(&v);
        *self.toaster_view.borrow_mut() = Some(v);
    }

    fn bring_toaster_to_front(&self) {
        if let Some(ref tv) = *self.toaster_view.borrow() {
            self.window.bringSubviewToFront(tv);
        }
    }

    #[allow(static_mut_refs)]
    fn coordinator() -> &'static Self {
        unsafe {
            COORDINATOR
                .as_ref()
                .expect("RootCoordinator not initialised")
        }
    }
}

// ---------------------------------------------------------------------------
// FFI entry points (called from Swift — keep #[no_mangle])
// ---------------------------------------------------------------------------

/// Called from Swift's AppDelegate to boot the Rust coordinator.
/// `window` is a +1 retained `UIWindow *`.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_start_root_coordinator(window: *mut c_void) {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let w: Retained<UIWindow> = unsafe { Retained::retain(window as *mut UIWindow).unwrap() };
    let rc = RootCoordinator::new(mtm, w);
    unsafe { COORDINATOR = Some(rc) };
    RootCoordinator::coordinator().start();
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the metal view from a VC — thin helper used by push_terminal.
unsafe fn bt_ios_vc_metal_view(vc_ptr: *mut std::ffi::c_void) -> *mut std::ffi::c_void {
    if vc_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let vc_obj = vc_ptr as *mut objc2::runtime::AnyObject;
    let mv: *const crate::metal_view::BtIosMetalInputView =
        unsafe { objc2::msg_send![&*vc_obj, btIosMetalView] };
    mv as *mut std::ffi::c_void
}

fn hosts_vm_lock() -> std::sync::MutexGuard<'static, hosts_vm::HostsVM> {
    match hosts_vm::VM.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn hosts_vm_load_from_store() {
    let json = hosts_store::list_snapshot_json();
    hosts_vm_lock().set_entries_from_snapshot(&json);
}

fn connect_form_vm_lock() -> std::sync::MutexGuard<'static, connect_form_vm::ConnectFormVM> {
    match connect_form_vm::VM.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn credential_for_terminal(
    auth: &bedterm_app::hosts::model::AuthMethod,
) -> bedterm_app::credential::HostCredential {
    match auth {
        bedterm_app::hosts::model::AuthMethod::Password(pwd) => {
            bedterm_app::credential::HostCredential::Password {
                password: pwd.clone(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Callback trampolines
// ---------------------------------------------------------------------------

unsafe extern "C" fn add_trampoline(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    this.present_host_form();
}

unsafe extern "C" fn connect_trampoline(ctx: *mut c_void, id: *const c_char) {
    if ctx.is_null() || id.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    if let Ok(s) = unsafe { std::ffi::CStr::from_ptr(id) }.to_str() {
        this.run_connect(s);
    }
}

unsafe extern "C" fn disconnect_trampoline(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    this.disconnect_session();
}

unsafe extern "C" fn form_save_trampoline(ctx: *mut c_void, json: *const c_char) {
    if ctx.is_null() || json.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    let s = unsafe { std::ffi::CStr::from_ptr(json) }
        .to_str()
        .unwrap_or("");
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(s) {
        if let (Some(id), Some(blob)) = (
            data.get("id").and_then(|v| v.as_str()),
            serde_json::to_string(&data).ok(),
        ) {
            let ic = CString::new(id).unwrap_or_default();
            hosts_store::save(
                unsafe { std::ffi::CStr::from_ptr(ic.as_ptr()) }
                    .to_str()
                    .unwrap_or(""),
                blob.as_bytes(),
            );
            this.refresh_hosts_list();
        }
    }
}

unsafe extern "C" fn back_trampoline(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    this.disconnect_session();
    if let Some(ref nav) = *this.nav.borrow() {
        let _: () = unsafe { msg_send![&**nav, popToRootViewControllerAnimated: true] };
    }
}

#[allow(dead_code)]
unsafe extern "C" fn settings_done_trampoline(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    if let Some(ref nav) = *this.nav.borrow() {
        unsafe {
            let _: () = msg_send![&**nav, dismissViewControllerAnimated: true, completion: std::ptr::null::<c_void>()];
        }
    }
}

unsafe extern "C" fn form_done_trampoline(
    ctx: *mut c_void,
    _id: *const c_char,
    _connect_now: bool,
) {
    if ctx.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    this.refresh_hosts_list();
    if let Some(ref nav) = *this.nav.borrow() {
        unsafe {
            let _: () = msg_send![&**nav, popViewControllerAnimated: true];
        }
    }
}

unsafe extern "C" fn form_cancel_trampoline(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    if let Some(ref nav) = *this.nav.borrow() {
        unsafe {
            let _: () = msg_send![&**nav, popViewControllerAnimated: true];
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn mismatch_trust_trampoline(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    if let Some(ref nav) = *this.nav.borrow() {
        unsafe {
            let _: () = msg_send![&**nav, dismissViewControllerAnimated: true, completion: std::ptr::null::<c_void>()];
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn mismatch_reject_trampoline(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let this = &*(ctx as *const RootCoordinator);
    if let Some(ref nav) = *this.nav.borrow() {
        unsafe {
            let _: () = msg_send![&**nav, dismissViewControllerAnimated: true, completion: std::ptr::null::<c_void>()];
        }
    }
}

unsafe extern "C" fn noop_completion(
    _: *mut c_void,
    _: bedterm_app::ssh_bridge::BtSSHResultCode,
    _: *const c_void,
    _: i32,
) {
}
