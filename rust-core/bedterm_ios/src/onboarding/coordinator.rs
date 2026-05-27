//! `BtIosOnboardingFlowVC` — Rust-owned `UINavigationController` that
//! drives the full R10 onboarding flow.
//!
//! After W23d, the Swift host no longer knows the step sequence. It calls
//! [`bt_ios_create_onboarding_flow_vc`] (see `ffi::onboarding`) and installs
//! the returned VC as its window root. The coordinator owns:
//!
//! - an [`OnboardingState`](super::state::OnboardingState) instance,
//! - a `*mut c_void` host context + completion callback fired once when
//!   the user finishes the final step, and
//! - the four per-step `UIViewController` instances (created lazily and
//!   pushed onto `self` as the user advances).
//!
//! The terminal step (LocalPermission) asks Swift to run the
//! `LocalNetworkPrewarmer` Bonjour probe via the
//! `bt_swift_request_local_network` cross-FFI; on its completion callback
//! the coordinator marks onboarding completed
//! (`bt_swift_onboarding_set_completed(true)`) and fires the host's
//! `on_completed` callback.

#![cfg(target_os = "ios")]

use crate::onboarding::host_kind_vc::create_host_kind_vc;
use crate::onboarding::local_permission_vc::create_local_permission_vc;
use crate::onboarding::location_vc::create_location_vc;
use crate::onboarding::mac_tutorial_vc::create_mac_tutorial_vc;
use crate::onboarding::state::{HostKind, Location, OnboardingState, Step};
use crate::onboarding::BtIosOnboardingFlowCompletedCallback;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_foundation::MainThreadMarker;
use objc2_ui_kit::{UINavigationController, UIViewController};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

// Swift-side bridges the coordinator reaches out to.
//
// `bt_swift_request_local_network(ctx, completion)` triggers the
// Bonjour-based Local Network permission probe and, when iOS resolves it,
// invokes `completion(ctx)` on the main queue.
//
// `bt_swift_onboarding_set_completed(true)` flips the persistent flag the
// `RootCoordinator` reads on launch.
extern "C" {
    fn bt_swift_request_local_network(
        ctx: *mut c_void,
        completion: unsafe extern "C" fn(*mut c_void),
    );
    fn bt_swift_onboarding_set_completed(value: bool);
}

/// Heap-allocated coordinator pinned to the flow VC. Holds the entire
/// onboarding state machine + the host's completion callback. Lifetime is
/// scoped to the flow VC: the subclass's `dealloc` frees it.
pub(crate) struct Coordinator {
    pub(crate) state: RefCell<OnboardingState>,
    /// Set by the FFI constructor; fired once when the flow ends.
    pub(crate) on_completed: Cell<Option<BtIosOnboardingFlowCompletedCallback>>,
    /// SAFETY: never dereffed on the Rust side; ownership and lifetime
    /// are the Swift host's responsibility.
    pub(crate) host_ctx: Cell<*mut c_void>,
    /// Weak handle to the flow VC; used to push step VCs and to look up
    /// the coordinator pointer from the per-step C trampolines.
    pub(crate) nav: RefCell<Option<*const BtIosOnboardingFlowVC>>,
}

impl Coordinator {
    fn new() -> Self {
        Self {
            state: RefCell::new(OnboardingState::new()),
            on_completed: Cell::new(None),
            host_ctx: Cell::new(std::ptr::null_mut()),
            nav: RefCell::new(None),
        }
    }
}

#[derive(Default)]
pub struct Ivars {
    /// Owned `Box<Coordinator>` leaked into a raw pointer for ivar storage.
    /// Freed in the class's `dealloc`.
    ///
    /// SAFETY: read-only outside `init` / `dealloc`; the box itself is
    /// `!Send` but the VC is `MainThreadOnly`, so the pointer is only
    /// touched from the main thread.
    coordinator: Cell<*mut Coordinator>,
}

unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// `BtIosOnboardingFlowVC` — UINavigationController subclass that owns
    /// the onboarding state machine and drives step transitions.
    ///
    /// Ivar cleanup happens through `Drop for Ivars` (objc2 invokes it
    /// from the synthesised `dealloc`), so the heap-allocated coordinator
    /// is freed exactly when the navigation controller is.
    #[unsafe(super(UINavigationController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosOnboardingFlowVC"]
    #[ivars = Ivars]
    pub struct BtIosOnboardingFlowVC;

    impl BtIosOnboardingFlowVC {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let coord = Box::into_raw(Box::new(Coordinator::new()));
            let this = this.set_ivars(Ivars {
                coordinator: Cell::new(coord),
            });
            unsafe { msg_send![super(this), init] }
        }
    }
);

impl Drop for Ivars {
    fn drop(&mut self) {
        let ptr = self.coordinator.get();
        if !ptr.is_null() {
            // SAFETY: pointer originated from `Box::into_raw` in `init`,
            // and `Drop` runs exactly once per VC instance.
            unsafe {
                drop(Box::from_raw(ptr));
            }
            self.coordinator.set(std::ptr::null_mut());
        }
    }
}

impl BtIosOnboardingFlowVC {
    pub(crate) fn coordinator(&self) -> &Coordinator {
        let ptr = self.ivars().coordinator.get();
        // SAFETY: non-null for the lifetime of the VC (set in init,
        // cleared in dealloc only after no further methods can run).
        unsafe { &*ptr }
    }

    fn set_callback(
        &self,
        on_completed: Option<BtIosOnboardingFlowCompletedCallback>,
        ctx: *mut c_void,
    ) {
        let c = self.coordinator();
        c.on_completed.set(on_completed);
        c.host_ctx.set(ctx);
        *c.nav.borrow_mut() = Some(self as *const _);
    }

    fn push_next(&self, vc_raw: *mut c_void) {
        if vc_raw.is_null() {
            return;
        }
        // SAFETY: `vc_raw` is a +1 retained UIViewController pointer from
        // one of the per-step Rust constructors. Bring it back into Rust
        // ownership, push it onto self (which retains), then drop the
        // local Retained which balances the +1.
        let vc: Retained<UIViewController> =
            unsafe { Retained::from_raw(vc_raw as *mut UIViewController).expect("non-null") };
        unsafe {
            let _: () = msg_send![self, pushViewController: &*vc, animated: true];
        }
    }

    fn finish(&self) {
        let c = self.coordinator();
        // Persist completion via Swift bridge (no-op in unit tests since
        // the symbol resolves to a stub that updates UserDefaults).
        unsafe { bt_swift_onboarding_set_completed(true) };
        let cb = c.on_completed.get();
        let ctx = c.host_ctx.get();
        if let Some(cb) = cb {
            unsafe { cb(ctx) };
        }
    }
}

// ── Per-step C trampolines ───────────────────────────────────────────────
//
// Each callback recovers the coordinator from the ctx pointer (which is
// `*const BtIosOnboardingFlowVC`), mutates state, and pushes the next VC.

unsafe extern "C" fn host_kind_choice_cb(ctx: *mut c_void, choice: i32) {
    if ctx.is_null() {
        return;
    }
    let flow = unsafe { &*(ctx as *const BtIosOnboardingFlowVC) };
    let Some(kind) = HostKind::from_choice(choice) else {
        return;
    };
    flow.coordinator().state.borrow_mut().select_host_kind(kind);
    let location_vc = unsafe { create_location_vc(Some(location_choice_cb), ctx) };
    flow.push_next(location_vc);
}

unsafe extern "C" fn location_choice_cb(ctx: *mut c_void, choice: i32) {
    if ctx.is_null() {
        return;
    }
    let flow = unsafe { &*(ctx as *const BtIosOnboardingFlowVC) };
    let Some(loc) = Location::from_choice(choice) else {
        return;
    };
    flow.coordinator().state.borrow_mut().select_location(loc);
    let host = flow
        .coordinator()
        .state
        .borrow()
        .host_kind
        .expect("host_kind set before location step");
    match OnboardingState::next_step_after_location(host, loc) {
        Step::MacTutorial => {
            let vc = unsafe { create_mac_tutorial_vc(Some(mac_tutorial_continue_cb), ctx) };
            flow.push_next(vc);
        }
        Step::LocalPermission => {
            let vc = unsafe {
                create_local_permission_vc(
                    Some(local_permission_continue_cb),
                    ctx,
                    OnboardingState::is_remote_terminator(loc),
                )
            };
            flow.push_next(vc);
        }
        Step::Location => {
            // Unreachable per state machine. Defensive no-op.
        }
    }
}

unsafe extern "C" fn mac_tutorial_continue_cb(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let flow = unsafe { &*(ctx as *const BtIosOnboardingFlowVC) };
    let loc = flow
        .coordinator()
        .state
        .borrow()
        .location
        .expect("location set before mac-tutorial step");
    match OnboardingState::next_step_after_mac_tutorial(loc) {
        Some(Step::LocalPermission) => {
            let vc = unsafe {
                create_local_permission_vc(
                    Some(local_permission_continue_cb),
                    ctx,
                    OnboardingState::is_remote_terminator(loc),
                )
            };
            flow.push_next(vc);
        }
        None => {
            // Other host + remote → no permission needed; finish directly.
            flow.finish();
        }
        Some(_) => {}
    }
}

unsafe extern "C" fn local_permission_continue_cb(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let flow = unsafe { &*(ctx as *const BtIosOnboardingFlowVC) };
    let loc = flow.coordinator().state.borrow().location;
    if matches!(loc, Some(Location::SameWifi)) {
        // Kick the Bonjour probe; finish on its completion. `ctx` is the
        // flow VC pointer; same value flows into the completion thunk.
        unsafe {
            bt_swift_request_local_network(ctx, local_permission_probe_done_cb);
        }
    } else {
        flow.finish();
    }
}

unsafe extern "C" fn local_permission_probe_done_cb(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    let flow = unsafe { &*(ctx as *const BtIosOnboardingFlowVC) };
    flow.finish();
}

/// Construct the flow VC, wire its initial child, and return it as an
/// opaque +1 retained `UIViewController *` (the caller releases via
/// `bt_ios_release_vc`).
pub(crate) unsafe fn create_flow_vc(
    on_completed: Option<BtIosOnboardingFlowCompletedCallback>,
    ctx: *mut c_void,
) -> *mut c_void {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let nav: Retained<BtIosOnboardingFlowVC> =
        unsafe { msg_send![mtm.alloc::<BtIosOnboardingFlowVC>(), init] };
    nav.set_callback(on_completed, ctx);

    let nav_ptr = (&*nav as *const BtIosOnboardingFlowVC) as *mut c_void;
    let host_kind_raw = unsafe { create_host_kind_vc(Some(host_kind_choice_cb), nav_ptr) };
    if !host_kind_raw.is_null() {
        let host_kind: Retained<UIViewController> = unsafe {
            Retained::from_raw(host_kind_raw as *mut UIViewController).expect("non-null")
        };
        let arr = objc2_foundation::NSArray::from_retained_slice(&[host_kind]);
        unsafe {
            let _: () = msg_send![&*nav, setViewControllers: &*arr, animated: false];
        }
    }
    // Mark `_` so unused-binding lint is happy when the optional branch
    // above no-ops (e.g. constructor failure).
    let _ = &*nav as &AnyObject;
    Retained::into_raw(nav) as *mut c_void
}
