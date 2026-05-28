//! UIKit-coupled `BtIosBlockListViewController` class definition.
//!
//! Extracted from `block_list/mod.rs` so the pure submodules (`layout`,
//! `scroll`, `selection`, `sticky`, `source`) stay host-compilable for
//! `cargo test -p bedterm_ios` on macOS hosts. The class itself only
//! makes sense on iOS where UIKit is linkable.

use super::layout::{compute_block_ranges, content_height_pt, HEADER_HEIGHT_PT};
use super::scroll::{self, BlockListScrollState};
use super::selection::{block_hit_test, copy_to_pasteboard, BlockSelectionState};
use super::source::{BlockSnapshot, BlockSource, EmptyBlockSource};
use super::sticky::build_sticky_descriptor;

use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_quartz_core::CADisplayLink;
use objc2_ui_kit::{
    UIGestureRecognizer, UIGestureRecognizerState, UILongPressGestureRecognizer,
    UIPanGestureRecognizer, UIView, UIViewController,
};
use std::cell::{Cell, RefCell};

// `CACurrentMediaTime()` — local extern to avoid a wider dependency.
extern "C" {
    fn CACurrentMediaTime() -> f64;
}

fn current_media_time() -> f64 {
    unsafe { CACurrentMediaTime() }
}

/// Per-instance state. All accesses on the main thread.
pub struct Ivars {
    /// Weak ref to the Swift-side `TerminalSession` (an ObjC-bridged
    /// `NSObject`). The Swift host owns the session; this VC just
    /// observes it for selection / copy / scroll feedback. Set once
    /// from Swift at attach time; cleared on detach.
    ///
    /// Legacy field, retained for FFI compatibility while the Swift
    /// host still constructs this VC directly.
    ///
    /// SAFETY: only deref when `self.session.get()` is non-null AND
    /// the Swift host has not detached the session. Main-thread only
    /// (`MainThreadOnly` class).
    session: Cell<*const AnyObject>,
    /// Weak ref to the shared `BtIosMetalInputView`. The parent VC
    /// (`RsTerminalViewController`) owns the metal view through its
    /// `Ivars::view1` + the subview tree; this block-list VC just
    /// observes it for cell-metric queries.
    ///
    /// SAFETY: only deref when `self.shared_metal_view.get()` is
    /// non-null AND the parent VC is still alive. Main-thread only.
    shared_metal_view: Cell<*const AnyObject>,

    /// Transparent gesture-overlay view that scrolls in lockstep. No
    /// glyph drawing — it just hosts the selection highlight layer and
    /// catches taps. Matches Swift's `contentView`.
    content_view: RefCell<Option<Retained<UIView>>>,

    /// Pan recogniser for the scroll surface.
    pan_gr: RefCell<Option<Retained<UIPanGestureRecognizer>>>,
    /// Long-press recogniser feeding `selection`.
    long_press_gr: RefCell<Option<Retained<UILongPressGestureRecognizer>>>,
    /// CADisplayLink driving momentum + per-frame layout while a
    /// running block exists.
    display_link: RefCell<Option<Retained<CADisplayLink>>>,

    /// Scroll/anchor state machine (pure values, see `scroll.rs`).
    scroll: RefCell<BlockListScrollState>,
    /// Long-press selection state machine.
    selection: RefCell<BlockSelectionState>,

    /// Block source vtable. None until `set_block_source` is called by W6.
    block_source: RefCell<Box<dyn BlockSource>>,

    /// Atlas-derived cell metrics (refreshed each layout pass).
    row_height_pt: Cell<CGFloat>,
    cell_width_pt: Cell<CGFloat>,

    /// Cache of the most recent layout — used by `selection_long_press`
    /// hit-tests without recomputing.
    cached_blocks: RefCell<Vec<BlockSnapshot>>,

    /// Last bounds we laid out at — used to short-circuit redundant
    /// `viewWillLayoutSubviews` calls.
    last_layout_size: Cell<CGSize>,
}

impl Default for Ivars {
    fn default() -> Self {
        Self {
            session: Cell::new(std::ptr::null()),
            shared_metal_view: Cell::new(std::ptr::null()),
            content_view: RefCell::new(None),
            pan_gr: RefCell::new(None),
            long_press_gr: RefCell::new(None),
            display_link: RefCell::new(None),
            scroll: RefCell::new(BlockListScrollState::default()),
            selection: RefCell::new(BlockSelectionState::default()),
            block_source: RefCell::new(Box::new(EmptyBlockSource)),
            row_height_pt: Cell::new(18.0),
            cell_width_pt: Cell::new(9.0),
            cached_blocks: RefCell::new(Vec::new()),
            last_layout_size: Cell::new(CGSize::default()),
        }
    }
}

// SAFETY: only accessed on the main thread (MainThreadOnly class).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    /// Rust-backed child VC for `.blockList` display mode. Mirrors
    /// Swift's `BlockListContainerViewController` 1:1.
    #[unsafe(super(UIViewController))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosBlockListViewController"]
    #[ivars = Ivars]
    pub struct BtIosBlockListViewController;

    impl BtIosBlockListViewController {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send![super(this), init] }
        }

        #[unsafe(method(viewDidLoad))]
        fn view_did_load(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidLoad] };
            let mtm = unsafe { MainThreadMarker::new_unchecked() };

            // Transparent content view — gesture overlay only.
            let content: Retained<UIView> = unsafe {
                let alloc: Allocated<UIView> = mtm.alloc::<UIView>();
                msg_send![alloc, initWithFrame: CGRect::default()]
            };
            unsafe {
                let _: () = msg_send![&*content, setUserInteractionEnabled: false];
            }
            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addSubview: &*content] };
            }
            *self.ivars().content_view.borrow_mut() = Some(content);

            // Pan gesture — drives the scroll state machine.
            let pan: Retained<UIPanGestureRecognizer> = unsafe {
                let alloc = UIPanGestureRecognizer::alloc(mtm);
                let target: *const AnyObject = self as *const Self as *const AnyObject;
                msg_send![
                    alloc,
                    initWithTarget: target,
                    action: sel!(handlePan:),
                ]
            };
            unsafe {
                let _: () = msg_send![&*pan, setCancelsTouchesInView: false];
            }
            pan.setMaximumNumberOfTouches(1);
            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addGestureRecognizer: &*pan] };
            }
            *self.ivars().pan_gr.borrow_mut() = Some(pan);

            // Long-press → selection.
            let lp: Retained<UILongPressGestureRecognizer> = unsafe {
                let alloc = UILongPressGestureRecognizer::alloc(mtm);
                let target: *const AnyObject = self as *const Self as *const AnyObject;
                msg_send![
                    alloc,
                    initWithTarget: target,
                    action: sel!(selectionLongPress:),
                ]
            };
            lp.setMinimumPressDuration(0.4);
            if let Some(view) = self.view() {
                let _: () = unsafe { msg_send![&*view, addGestureRecognizer: &*lp] };
            }
            *self.ivars().long_press_gr.borrow_mut() = Some(lp);
        }

        #[unsafe(method(viewWillLayoutSubviews))]
        fn view_will_layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), viewWillLayoutSubviews] };
            let Some(view) = self.view() else {
                return;
            };
            let bounds: CGRect = unsafe { msg_send![&*view, bounds] };
            if let Some(cv) = self.ivars().content_view.borrow().as_ref() {
                let _: () = unsafe { msg_send![&**cv, setFrame: bounds] };
            }
            // Update viewport metric inside the scroll state.
            self.ivars().scroll.borrow_mut().viewport_height = bounds.size.height;
            if size_changed(self.ivars().last_layout_size.get(), bounds.size) {
                self.ivars().last_layout_size.set(bounds.size);
                self.refresh_inner();
            }
        }

        #[unsafe(method(handlePan:))]
        fn handle_pan(&self, gr: &UIPanGestureRecognizer) {
            let state: UIGestureRecognizerState = unsafe { msg_send![gr, state] };
            let Some(view) = self.view() else {
                return;
            };
            let now = current_media_time();
            match state {
                UIGestureRecognizerState::Began => {
                    self.ivars().scroll.borrow_mut().begin_pan(now);
                }
                UIGestureRecognizerState::Changed => {
                    let t: CGPoint = unsafe { msg_send![gr, translationInView: &*view] };
                    self.ivars().scroll.borrow_mut().update_pan(t.y, now);
                    self.push_layout_to_metal_view();
                }
                UIGestureRecognizerState::Ended | UIGestureRecognizerState::Cancelled => {
                    self.ivars().scroll.borrow_mut().end_pan(now);
                    self.update_display_link();
                }
                _ => {}
            }
        }

        #[unsafe(method(displayTick))]
        fn display_tick(&self) {
            let now = current_media_time();
            let exhausted = self.ivars().scroll.borrow_mut().advance_momentum(now);
            // Per Swift parity: re-walk content size each tick so running
            // blocks (whose body height grows mid-frame) update.
            self.update_content_size();
            self.ivars().scroll.borrow_mut().apply_scroll_position(false);
            self.push_layout_to_metal_view();
            if exhausted {
                self.update_display_link();
            }
        }

        #[unsafe(method(selectionLongPress:))]
        fn selection_long_press(&self, gr: &UIGestureRecognizer) {
            let state: UIGestureRecognizerState = unsafe { msg_send![gr, state] };
            let Some(cv) = self.ivars().content_view.borrow().clone() else {
                return;
            };
            let pt: CGPoint = unsafe { msg_send![gr, locationInView: &*cv] };
            // contentView is viewport-sized → translate to content space.
            let scroll_y = self.ivars().scroll.borrow().content_offset_y;
            let point_in_content = (pt.x, pt.y + scroll_y);

            match state {
                UIGestureRecognizerState::Began => {
                    let blocks = self.ivars().cached_blocks.borrow().clone();
                    let row_h = self.ivars().row_height_pt.get();
                    let ranges = compute_block_ranges(&blocks, HEADER_HEIGHT_PT, row_h);
                    // cols = container_width / cell_width — approximate.
                    let cw = self.ivars().cell_width_pt.get();
                    let cont_w = self.ivars().selection.borrow().container_width_pt;
                    let cols = if cw > 0.0 { (cont_w / cw) as i32 } else { 0 };
                    if let Some(hit) =
                        block_hit_test(&ranges, point_in_content, HEADER_HEIGHT_PT, row_h, cols)
                    {
                        self.ivars().selection.borrow_mut().begin(point_in_content, hit);
                    } else {
                        self.ivars().selection.borrow_mut().cancel();
                    }
                }
                UIGestureRecognizerState::Changed => {
                    self.ivars().selection.borrow_mut().extend(point_in_content);
                }
                UIGestureRecognizerState::Ended => {
                    if let Some((_block_id, _range)) =
                        self.ivars().selection.borrow_mut().finish()
                    {
                        // Block-text extraction lives behind a private
                        // `Terminal` accessor in `bedterm_core` with no
                        // public C export yet — copy an empty payload
                        // for now so the gesture still fires.
                        copy_to_pasteboard("");
                    }
                }
                UIGestureRecognizerState::Cancelled | UIGestureRecognizerState::Failed => {
                    self.ivars().selection.borrow_mut().cancel();
                }
                _ => {}
            }
        }
    }
);

impl BtIosBlockListViewController {
    /// Public Rust constructor — mirrors Swift's
    /// `init(session:sharedMetalView:)`. Both pointers are stored
    /// weakly; the parent owns lifetimes.
    pub fn new(
        mtm: MainThreadMarker,
        session: *const AnyObject,
        shared_metal_view: *const AnyObject,
    ) -> Retained<Self> {
        let this: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), init] };
        this.ivars().session.set(session);
        this.ivars().shared_metal_view.set(shared_metal_view);
        this
    }

    /// Install / swap the block source. Called by the parent VC once
    /// the Rust `TerminalSession` is wired (W6).
    pub fn set_block_source(&self, src: Box<dyn BlockSource>) {
        *self.ivars().block_source.borrow_mut() = src;
        self.refresh();
    }

    /// Mirrors Swift's `refresh()` — re-reads block data, recomputes
    /// content size, re-applies the scroll anchor, repaints.
    pub fn refresh(&self) {
        self.refresh_inner();
    }

    fn refresh_inner(&self) {
        // Snapshot blocks once per refresh so the cached layout matches
        // what's pushed to the renderer.
        let count = self.ivars().block_source.borrow().block_count();
        let mut blocks: Vec<BlockSnapshot> = Vec::with_capacity(count);
        for i in 0..count {
            if let Some(b) = self.ivars().block_source.borrow().block_at(i) {
                blocks.push(b);
            }
        }
        let mut scroll = self.ivars().scroll.borrow_mut();
        if blocks.len() > scroll.last_block_count {
            scroll.scroll_position = scroll::ScrollPosition::FollowsBottom;
        }
        scroll.last_block_count = blocks.len();
        drop(scroll);
        *self.ivars().cached_blocks.borrow_mut() = blocks;
        self.update_content_size();
        self.ivars()
            .scroll
            .borrow_mut()
            .apply_scroll_position(false);
        self.push_layout_to_metal_view();
        self.update_display_link();
        // Push container width + cell metrics into the selection state.
        let cont_w = self
            .view()
            .map(|v| {
                let b: CGRect = unsafe { msg_send![&*v, bounds] };
                b.size.width
            })
            .unwrap_or(0.0);
        self.ivars().selection.borrow_mut().update_metrics(
            self.ivars().cell_width_pt.get(),
            self.ivars().row_height_pt.get(),
            cont_w,
            f64::from(crate::block_panel_style::CELL_LEFT_INSET_PT),
        );
    }

    fn update_content_size(&self) {
        let row_h = self.ivars().row_height_pt.get();
        let blocks = self.ivars().cached_blocks.borrow();
        let h = content_height_pt(&blocks, HEADER_HEIGHT_PT, row_h);
        self.ivars().scroll.borrow_mut().content_height = h;
    }

    /// Push the per-frame layout descriptor table to the shared metal
    /// view. The Rust↔C bridge for `BtBlockLayoutEntry` /
    /// `BtBlockHeaderEntry` lives in `BedTermIOS` and isn't yet
    /// exposed to this crate — wiring lands in W7. We do still compute
    /// the ranges + sticky descriptor so the math is exercised.
    fn push_layout_to_metal_view(&self) {
        let blocks = self.ivars().cached_blocks.borrow();
        let row_h = self.ivars().row_height_pt.get();
        let ranges = compute_block_ranges(&blocks, HEADER_HEIGHT_PT, row_h);
        let scroll_y = self.ivars().scroll.borrow().content_offset_y;
        let _sticky = build_sticky_descriptor(&ranges, scroll_y, HEADER_HEIGHT_PT);
        // TODO(post-block-list-painter-ffi): assemble `BtBlockLayoutEntry` /
        // `BtBlockHeaderEntry` arrays and call
        // `TerminalMetalUIView::updateBlockLayout` via the
        // `shared_metal_view` pointer. The C bridge for those
        // descriptor structs is not exposed from `bedterm_core` to
        // this crate yet; landing it requires a follow-up that mirrors
        // the in-Swift `BlockListPainter` math through a new FFI
        // surface. For W7 we still push `setNeedsDisplay` below so
        // scroll-induced repaints fire, matching Swift parity.
        let mv = self.ivars().shared_metal_view.get();
        if !mv.is_null() {
            // Best-effort: poke `setNeedsDisplay` so the renderer at
            // least picks up scroll-induced changes once the FFI bridge
            // lands. No-op until then on the descriptor side.
            unsafe {
                let _: () = msg_send![&*mv, setNeedsDisplay];
            }
        }
    }

    /// Drive the CADisplayLink lifecycle. Active whenever momentum is
    /// running or any block reports `is_running`. Matches Swift's
    /// `updateDisplayLink`.
    fn update_display_link(&self) {
        let needs = self.ivars().scroll.borrow().scroll_physics.is_some()
            || self
                .ivars()
                .cached_blocks
                .borrow()
                .iter()
                .any(|b| b.is_running);
        let has_link = self.ivars().display_link.borrow().is_some();
        if needs && !has_link {
            let link: Retained<CADisplayLink> =
                unsafe { CADisplayLink::displayLinkWithTarget_selector(self, sel!(displayTick)) };
            unsafe {
                let runloop = objc2_foundation::NSRunLoop::mainRunLoop();
                link.addToRunLoop_forMode(&runloop, objc2_foundation::NSRunLoopCommonModes);
            }
            *self.ivars().display_link.borrow_mut() = Some(link);
        } else if !needs && has_link {
            if let Some(link) = self.ivars().display_link.borrow_mut().take() {
                link.invalidate();
            }
        }
    }

    /// Public teardown — mirrors Swift's `teardown()`. Must be called
    /// by the parent VC / SwiftUI representable's dismantle hook
    /// because the display-link → self retain cycle would otherwise
    /// keep this VC alive forever.
    pub fn teardown(&self) {
        if let Some(link) = self.ivars().display_link.borrow_mut().take() {
            link.invalidate();
        }
        if let Some(cv) = self.ivars().content_view.borrow_mut().take() {
            unsafe {
                let _: () = msg_send![&*cv, removeFromSuperview];
            }
        }
    }
}

fn size_changed(a: CGSize, b: CGSize) -> bool {
    (a.width - b.width).abs() > 0.5 || (a.height - b.height).abs() > 0.5
}
