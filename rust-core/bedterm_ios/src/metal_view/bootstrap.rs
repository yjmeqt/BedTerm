//! Construction-time plumbing for `BtIosMetalInputView`: the `Self::new`
//! Rust factory, the lazy renderer / `BtTerm` bootstrap, and the
//! external-term install path used by the session-owned-grid wiring.

use super::{BtIosMetalInputView, Ivars};
use crate::geometry::CGRect;
use bedterm_core::ffi::{bt_term_feed, bt_term_free, bt_term_new, BtTerm};
use bedterm_core::renderer::Renderer;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_metal::MTLDevice;

impl BtIosMetalInputView {
    /// Build a view bound to a Metal device. The caller retains ownership.
    /// Returns `None` if MTKView's designated initialiser bails (rare; happens
    /// in some headless test contexts).
    pub fn new(
        mtm: MainThreadMarker,
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Option<Retained<Self>> {
        let zero = CGRect::default();
        // Call MTKView's designated init `initWithFrame:device:` directly.
        // Going through `initWithFrame:` triggers MTKView's internal
        // `[self initWithFrame:device:nil]` fall-through, which re-enters
        // our class via objc_msgSend and ends up resetting partially-
        // initialised ivars — `bootstrap_renderer` then panics on
        // `self.ivars()`.
        let this: Option<Retained<Self>> =
            unsafe { msg_send![Self::alloc(mtm), initWithFrame: zero, device: device] };
        if let Some(ref v) = this {
            unsafe {
                let _: () = msg_send![&**v, setUserInteractionEnabled: true];
            }
            v.bootstrap_renderer(device);
        }
        this
    }

    /// Construct the Rust-side Metal renderer + owned `BtTerm`. Mirrors the
    /// Swift `RendererBridge` init: take a +1 retain on the device, build a
    /// fresh command queue from it, push both into `Renderer::from_ptrs`,
    /// then push the initial font / clear colour. On any failure (e.g. the
    /// pipeline / atlas init bails in a headless context) the renderer slot
    /// stays `None` and the view falls back to the legacy UIView-fill
    /// background paint.
    fn bootstrap_renderer(&self, device: &ProtocolObject<dyn MTLDevice>) {
        // +1 retain for the renderer (it adopts ownership through
        // `Device::from_ptr`).
        let device_ptr: *const std::ffi::c_void = unsafe {
            let r: Retained<ProtocolObject<dyn MTLDevice>> = Retained::retain(
                device as *const ProtocolObject<dyn MTLDevice>
                    as *mut ProtocolObject<dyn MTLDevice>,
            )
            .expect("device retain");
            Retained::into_raw(r) as *const std::ffi::c_void
        };
        // Build a fresh command queue and hand a +1 retain to the renderer.
        let queue_obj: Option<Retained<ProtocolObject<dyn objc2_metal::MTLCommandQueue>>> =
            unsafe { msg_send![device, newCommandQueue] };
        let Some(queue) = queue_obj else {
            // Failed to make a queue — drop our device retain to keep the
            // counts balanced and bail.
            unsafe {
                Retained::<ProtocolObject<dyn MTLDevice>>::from_raw(
                    device_ptr as *mut ProtocolObject<dyn MTLDevice>,
                );
            }
            return;
        };
        let queue_ptr: *const std::ffi::c_void = Retained::into_raw(queue) as *const _;

        let renderer = unsafe { Renderer::from_ptrs(device_ptr, queue_ptr) };
        let Some(mut renderer) = renderer else {
            // `from_ptrs` returned None — it never installed the retains, so
            // we must release them ourselves to keep refcounts balanced.
            unsafe {
                Retained::<ProtocolObject<dyn MTLDevice>>::from_raw(
                    device_ptr as *mut ProtocolObject<dyn MTLDevice>,
                );
                Retained::<ProtocolObject<dyn objc2_metal::MTLCommandQueue>>::from_raw(
                    queue_ptr as *mut ProtocolObject<dyn objc2_metal::MTLCommandQueue>,
                );
            }
            return;
        };

        // Push the initial font + clear colour, mirroring
        // `TerminalFontBootstrap` in Swift. Hard-coded 17pt @ 3x for now —
        // future wiring can refresh from the view's `contentScaleFactor`.
        renderer.set_font(17.0, 3.0);
        renderer.set_clear_color(0.0, 0.0, 0.0, 1.0);
        let cell_px = renderer.cell_pixel_size();
        self.ivars().cell_px.set(cell_px);

        // Own a small initial grid; `layoutSubviews` resizes it once the
        // view picks up a real bounds rect.
        // Only allocate an owned term when no external one was installed
        // ahead of bootstrap. The session-owned-grid path (decision A in
        // `terminal_session.rs`) calls `set_external_term` before mount;
        // tests + the standalone VC fall through to the owned grid.
        if self.ivars().term.get().is_null() {
            let term = bt_term_new(80, 24);
            self.ivars().term.set(term);
            self.ivars().grid_dim.set((80, 24));
            self.ivars().owns_term.set(true);
        }

        *self.ivars().renderer.borrow_mut() = Some(renderer);
    }

    /// Cell pixel size as reported by the renderer's glyph atlas. Returns
    /// `(0, 0)` if the renderer didn't initialise (headless tests).
    pub fn cell_pixel_size(&self) -> (u32, u32) {
        self.ivars().cell_px.get()
    }

    /// Feed raw terminal bytes into the owned `BtTerm`. Called by the
    /// FFI shim `bt_ios_view_feed_bytes` so Swift / tests can pump output
    /// into the grid before the rest of the wave-7 session wiring lands.
    pub fn feed_bytes(&self, bytes: &[u8]) {
        let term = self.ivars().term.get();
        if term.is_null() || bytes.is_empty() {
            return;
        }
        unsafe { bt_term_feed(term, bytes.as_ptr(), bytes.len()) };
        if let Some(cb) = self.ivars().on_term_feed.borrow().as_ref() {
            cb();
        }
    }

    /// Install an externally-owned `BtTerm` (the session decision-A path:
    /// the session owns the canonical grid, the view borrows it). Must
    /// be called before `bootstrap_renderer` allocates an internal grid
    /// — typically right after `BtIosMetalInputView::new`.
    ///
    /// The pointer is not freed in the view's `Drop`; the session
    /// guarantees a matching `bt_term_free`.
    #[allow(dead_code)]
    pub fn set_external_term(&self, term: *mut BtTerm, cols: u16, rows: u16) {
        // If we had previously allocated our own, free it now — the
        // external term replaces it.
        let prev = self.ivars().term.replace(term);
        if !prev.is_null() && self.ivars().owns_term.get() {
            unsafe { bt_term_free(prev) };
        }
        self.ivars().owns_term.set(false);
        self.ivars().grid_dim.set((cols, rows));
    }

    /// Raw `BtTerm *` the view paints from. Returns null when no term is
    /// installed yet (headless test). Callers must not free it. Reserved
    /// for the session-attach FFI introspection path.
    #[allow(dead_code)]
    pub fn term_ptr(&self) -> *mut BtTerm {
        self.ivars().term.get()
    }

    /// Install a callback fired after every `feed_bytes`. Used by the
    /// session to trigger a redraw + propagate to output observers.
    #[allow(dead_code)]
    pub fn set_on_term_feed(&self, cb: Box<dyn Fn()>) {
        *self.ivars().on_term_feed.borrow_mut() = Some(cb);
    }
}

// Silence the unused-import lint on `Ivars` in this file — it's referenced
// through `self.ivars()` (which doesn't go through the type name).
#[allow(dead_code)]
fn _force_ivars_use(_: &Ivars) {}
