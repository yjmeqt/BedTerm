//! `BtIosMetalInputView` — an `MTKView` subclass that conforms to `UITextInput`.
//!
//! Phase 1 doesn't render glyphs; it just clears the drawable to a background
//! colour derived from view2's text. All UIKit/Metal interop lives here.
//!
//! Notes on byte vs character handling: we store the text buffer as a UTF-8
//! `String` and use byte indices for the selection. iOS positions are NSString
//! (UTF-16) offsets, so non-ASCII input can show selection jitter under IME.
//! This is a documented Phase 1 limitation (see the design doc).
//!
//! The implementation is split across sibling files for navigability — every
//! `define_class!` selector here is a thin forwarder to a `do_*` helper that
//! lives in the file matching its concern (text input, IME, gestures, …).
//! Ivars + the class declaration stay in this file.

use crate::ime_preedit_overlay::ImePreeditOverlay;
use crate::metal_cursor_layer::MetalCursorLayer;
use crate::metal_selection_layer::{MetalSelectionLayer, SelectionRange};
use bedterm_app::geometry::{CGRect, CGSize};
use bedterm_app::scroll_physics::ScrollPhysics;
use bedterm_core::ffi::{bt_term_free, BtTerm};
use bedterm_core::renderer::Renderer;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{define_class, extern_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_foundation::{
    NSArray, NSComparisonResult, NSDictionary, NSInteger, NSObjectProtocol, NSRange, NSString,
};
use objc2_metal::MTLDevice;
use objc2_quartz_core::CADisplayLink;
use objc2_ui_kit::{
    NSWritingDirection, UIEvent, UIGestureRecognizer, UIKeyInput, UILongPressGestureRecognizer,
    UIPanGestureRecognizer, UIPressesEvent, UIResponder, UITextInput, UITextInputTraits,
    UITextLayoutDirection, UITextPosition, UITextRange, UITextSelectionRect,
    UITextStorageDirection, UIView,
};
use std::cell::{Cell, RefCell};

mod bootstrap;
mod gestures;
mod ime;
mod keys;
mod render;
mod sinks;
mod text_input;

pub use sinks::{OnResizeSink, OnSendSink};

// -------- MTKView (local extern declaration for iOS) ------------------------
//
// `objc2-metal-kit` 0.3 only exposes `MTKView` on macOS (it requires
// `objc2-app-kit`). The framework itself is linked unconditionally by the
// crate, so we can declare the class locally for our iOS subclass to inherit.

extern_class!(
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MTKView"]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct MTKView;
);

// -------- Ivars -------------------------------------------------------------

#[derive(Default)]
#[allow(dead_code)]
pub struct Ivars {
    /// Text buffer rendered into NSString on demand. UTF-8.
    pub(super) text: RefCell<String>,
    /// Byte indices into `text`.
    pub(super) selected_start: Cell<usize>,
    pub(super) selected_end: Cell<usize>,
    /// Marked range (active IME composition). `-1` = none.
    pub(super) marked_start: Cell<i64>,
    pub(super) marked_end: Cell<i64>,
    /// Weak ref to UIKit's `UITextInputDelegate` (assigned by UIKit when
    /// the view becomes first responder). UIKit owns the delegate object;
    /// we just hand the pointer back from `inputDelegate` and forward
    /// notifications to it. Set by UIKit via `setInputDelegate:`; cleared
    /// by UIKit when the view resigns first responder.
    ///
    /// SAFETY: never deref unless `self.input_delegate.get()` is non-null
    /// AND we are still first responder (UIKit clears the delegate before
    /// it goes away). Main-thread only.
    pub(super) input_delegate: Cell<*const AnyObject>,
    /// Lazily initialised `UITextInputStringTokenizer` (caches once built).
    pub(super) tokenizer: RefCell<Option<Retained<NSObject>>>,
    /// Weak ref to the parent VC's `BtIosKeyboardCoordinator`. The VC
    /// strongly retains the coordinator in `Ivars::coordinator`; this view
    /// is one of the coordinator's two observed views, so the coordinator
    /// (and the VC) outlive every read here. Set once in the VC's
    /// `viewDidLoad` via `set_coordinator` after construction; never
    /// cleared.
    ///
    /// SAFETY: only deref when `self.coordinator.get()` is non-null AND we
    /// are on the main thread. Producers/consumers all run on the main
    /// thread by `MainThreadOnly` contract.
    pub(super) coordinator: Cell<*const AnyObject>,
    /// Weak raw pointer to the shared `ModeState`. The VC owns the
    /// authoritative `Rc<ModeState>` (in `Ivars::mode_state`) and clones
    /// distribute strong handles to the coordinator + HUD. This pointer
    /// is `Rc::as_ptr(&mode_state)` from the VC's clone, valid as long
    /// as any `Rc` clone is live — which is at least until the VC drops
    /// its own clone, and the VC drops *after* its subviews (UIKit
    /// teardown order). Set in `viewDidLoad` via `set_mode_state`.
    ///
    /// When non-null, `canBecomeFirstResponder` returns YES only in
    /// `InputMode::State2`.
    ///
    /// SAFETY: only deref when `self.mode_state.get()` is non-null AND we
    /// are on the main thread. The `ModeState` is `!Send + !Sync` so any
    /// dereference outside the main thread is UB regardless of nullity.
    pub(super) mode_state: Cell<*const crate::input_mode::ModeState>,

    // ---- Cursor / selection (H1 + H2) -------------------------------------
    /// Lazily-installed cursor layer. Mirrors Swift's `cursorLayer`.
    pub(super) cursor_layer: RefCell<Option<MetalCursorLayer>>,
    /// Lazily-installed selection overlay layer.
    pub(super) selection_layer: RefCell<Option<MetalSelectionLayer>>,
    /// Live selection range during a long-press drag, if any.
    pub(super) selection_range: Cell<Option<SelectionRange>>,
    /// Long-press recogniser handling selection.
    pub(super) selection_gr: RefCell<Option<Retained<UILongPressGestureRecognizer>>>,
    /// One-finger pan recogniser for terminal scroll.
    pub(super) pan_gr: RefCell<Option<Retained<UIPanGestureRecognizer>>>,
    /// Cached cell size in points — refreshed from the renderer / host VC.
    pub(super) cell_size: Cell<CGSize>,
    /// Cached grid dimensions.
    pub(super) cols: Cell<i32>,
    pub(super) rows: Cell<i32>,
    /// Cached cursor coords (in cells) — written from `set_cursor`.
    pub(super) cursor_col: Cell<i32>,
    pub(super) cursor_row: Cell<i32>,

    // ---- Scroll inertia (H4) ----------------------------------------------
    pub(super) display_link: RefCell<Option<Retained<CADisplayLink>>>,
    pub(super) scroll_physics: RefCell<Option<ScrollPhysics>>,
    pub(super) drag_accumulator: Cell<f64>,
    /// Most recent scroll-offset delta requested by inertia. Read by the
    /// host (or VC) to feed into `TerminalCore::scrollBy` in W6/W7.
    pub(super) pending_scroll_rows: Cell<i32>,

    // ---- IME preedit (H3) -------------------------------------------------
    pub(super) preedit_overlay: RefCell<Option<Retained<ImePreeditOverlay>>>,

    // ---- Replay / hardware-key sink (H4 / H5) -----------------------------
    /// Sink for emitted bytes (hardware-key encode, IME commit). The VC
    /// installs this in W6/W7 wiring; absent => bytes drop on the floor.
    pub(super) on_send: RefCell<Option<OnSendSink>>,
    /// Ctrl latch: when armed, the next ASCII letter emitted via
    /// `insert_text` / `emit_chip` is XOR-masked to `& 0x1F` (Ctrl-x byte)
    /// and the latch clears. Mirrors Swift `KeyBarState.ctrlPending`.
    pub(super) ctrl_pending: Cell<bool>,

    // ---- Glyph renderer + owned terminal grid -----------------------------
    /// Rust-side Metal renderer mirroring Swift's `RendererBridge`. Built
    /// lazily once the host hands us an `MTLDevice` (see `new`). `None`
    /// keeps headless test contexts green — they construct the view without
    /// a real Metal device.
    pub(super) renderer: RefCell<Option<Renderer>>,
    /// `BtTerm` handle backing this view's grid. Ownership is conditional
    /// on `owns_term` (set in `bootstrap_renderer` to true, cleared to
    /// false when an external term is installed via `set_external_term`):
    /// - `owns_term == true`: this `Cell` is the sole owner. `Ivars::drop`
    ///   calls `bt_term_free` on the pointer.
    /// - `owns_term == false`: an external (Swift-side) party owns the
    ///   grid and is responsible for freeing it. `Drop` skips the free.
    ///
    /// Either way, this is the only `*mut BtTerm` anywhere in the crate —
    /// do not introduce a parallel pointer.
    ///
    /// SAFETY: deref requires non-null AND main-thread. The pointee is
    /// `!Send + !Sync` by convention (the renderer treats it as
    /// main-thread-only). Replacing the pointer requires either
    /// `bt_term_free`ing the previous value (if owned) or asserting the
    /// external owner is still alive.
    pub(super) term: Cell<*mut BtTerm>,
    /// Cached cell pixel size returned by the renderer's atlas. Used to
    /// derive grid dimensions during `layoutSubviews`.
    pub(super) cell_px: Cell<(u32, u32)>,
    /// Most recent (cols, rows) we pushed into the terminal — debounces
    /// the resize FFI on layout passes that don't change the grid.
    pub(super) grid_dim: Cell<(u16, u16)>,
    /// Whether `term` was allocated by `bootstrap_renderer` (true) or
    /// installed externally via `set_external_term` (false). Only owned
    /// terms are freed in `Drop`.
    pub(super) owns_term: Cell<bool>,
    /// Callback fired after every `feed_bytes` so a host session can
    /// trigger a redraw + propagate output to observers without polling.
    /// Installed by `set_on_term_feed`. Optional.
    pub(super) on_term_feed: RefCell<Option<Box<dyn Fn()>>>,
    /// Callback fired from `layoutSubviews` whenever the renderer-derived
    /// (cols, rows) changes. Lets the Swift host push the new dims into
    /// `TerminalSession.resize` so the SSH PTY tracks the actual viewport.
    pub(super) on_resize: RefCell<Option<OnResizeSink>>,
}

// SAFETY: Only accessed on the main thread (MainThreadOnly class).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

impl Drop for Ivars {
    fn drop(&mut self) {
        // Drop the renderer first (releases its device + queue retains).
        *self.renderer.borrow_mut() = None;
        // Free the owned terminal grid (skipped for externally-installed
        // terms — the host session owns those and frees them when its
        // own lifetime ends).
        let term = self.term.replace(std::ptr::null_mut());
        if !term.is_null() && self.owns_term.get() {
            unsafe { bt_term_free(term) };
        }
    }
}

// -------- Class -------------------------------------------------------------

define_class!(
    /// `MTKView` subclass that paints terminal glyphs via the shared
    /// `bedterm_core::renderer::Renderer` and conforms to `UITextInput`
    /// so it can host a keyboard.
    #[unsafe(super(MTKView, UIView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosMetalInputView"]
    #[ivars = Ivars]
    pub struct BtIosMetalInputView;

    impl BtIosMetalInputView {
        // ---- Init ----------------------------------------------------------

        /// MTKView's designated initialiser is `initWithFrame:device:`.
        /// Override **only** this path — our Rust factory `Self::new` calls
        /// it directly so MTKView's internal `initWithFrame:` → `[self
        /// initWithFrame:device:]` fall-through never re-enters us with
        /// half-initialised ivars.
        #[unsafe(method_id(initWithFrame:device:))]
        fn init_with_frame_device(
            this: Allocated<Self>,
            frame: CGRect,
            device: Option<&ProtocolObject<dyn MTLDevice>>,
        ) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe {
                msg_send![super(this), initWithFrame: frame, device: device]
            }
        }

        // ---- Responder ------------------------------------------------------

        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            self.do_can_become_first_responder()
        }

        // ---- Layout / draw -------------------------------------------------

        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            self.do_layout_subviews()
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, rect: CGRect) {
            self.do_draw_rect(rect)
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &NSObject, event: Option<&UIEvent>) {
            self.do_touches_began(touches, event)
        }

        // ---- UITextInput: text / selection ----------------------------------

        #[unsafe(method_id(text))]
        fn get_text(&self) -> Retained<NSString> {
            self.do_get_text()
        }

        #[unsafe(method_id(selectedTextRange))]
        fn get_selected_text_range(&self) -> Option<Retained<UITextRange>> {
            self.do_get_selected_text_range()
        }

        #[unsafe(method(setSelectedTextRange:))]
        fn set_selected_text_range(&self, range: Option<&UITextRange>) {
            self.do_set_selected_text_range(range)
        }

        // ---- UITextInput: marked text ---------------------------------------

        #[unsafe(method_id(markedTextRange))]
        fn get_marked_text_range(&self) -> Option<Retained<UITextRange>> {
            self.do_get_marked_text_range()
        }

        #[unsafe(method_id(markedTextStyle))]
        fn get_marked_text_style(&self) -> Option<Retained<NSDictionary>> {
            None
        }

        #[unsafe(method(setMarkedTextStyle:))]
        fn set_marked_text_style(&self, _style: Option<&NSDictionary>) {
            // No styling support in Phase 1.
        }

        #[unsafe(method(setMarkedText:selectedRange:))]
        fn set_marked_text(&self, marked_text: Option<&NSString>, selected_range: NSRange) {
            self.do_set_marked_text(marked_text, selected_range)
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            self.do_unmark_text()
        }

        // ---- UITextInput: positions / ranges --------------------------------

        #[unsafe(method_id(beginningOfDocument))]
        fn beginning_of_document(&self) -> Retained<UITextPosition> {
            self.do_beginning_of_document()
        }

        #[unsafe(method_id(endOfDocument))]
        fn end_of_document(&self) -> Retained<UITextPosition> {
            self.do_end_of_document()
        }

        #[unsafe(method_id(textRangeFromPosition:toPosition:))]
        fn text_range_from(
            &self,
            from: &UITextPosition,
            to: &UITextPosition,
        ) -> Option<Retained<UITextRange>> {
            self.do_text_range_from(from, to)
        }

        #[unsafe(method_id(positionFromPosition:offset:))]
        fn position_from_offset(
            &self,
            position: &UITextPosition,
            offset: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            self.do_position_from_offset(position, offset)
        }

        #[unsafe(method_id(positionFromPosition:inDirection:offset:))]
        fn position_from_in_direction(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
            offset: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            self.do_position_from_in_direction(position, direction, offset)
        }

        #[unsafe(method(comparePosition:toPosition:))]
        fn compare_position(
            &self,
            a: &UITextPosition,
            b: &UITextPosition,
        ) -> NSComparisonResult {
            self.do_compare_position(a, b)
        }

        #[unsafe(method(offsetFromPosition:toPosition:))]
        fn offset_from(&self, a: &UITextPosition, b: &UITextPosition) -> NSInteger {
            self.do_offset_from(a, b)
        }

        // ---- UITextInput: input delegate / tokenizer ------------------------

        #[unsafe(method(inputDelegate))]
        fn get_input_delegate(&self) -> *const AnyObject {
            self.ivars().input_delegate.get()
        }

        #[unsafe(method(setInputDelegate:))]
        fn set_input_delegate(&self, delegate: *const AnyObject) {
            self.ivars().input_delegate.set(delegate);
        }

        #[unsafe(method_id(tokenizer))]
        fn get_tokenizer(&self) -> Retained<NSObject> {
            self.do_get_tokenizer()
        }

        // ---- UITextInput: extent / direction helpers ------------------------

        #[unsafe(method_id(positionWithinRange:farthestInDirection:))]
        fn position_within_range_farthest(
            &self,
            range: &UITextRange,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextPosition>> {
            self.do_position_within_range_farthest(range, direction)
        }

        #[unsafe(method_id(characterRangeByExtendingPosition:inDirection:))]
        fn character_range_by_extending(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextRange>> {
            self.do_character_range_by_extending(position, direction)
        }

        #[unsafe(method(baseWritingDirectionForPosition:inDirection:))]
        fn base_writing_direction(
            &self,
            _position: &UITextPosition,
            _direction: UITextStorageDirection,
        ) -> NSWritingDirection {
            NSWritingDirection::Natural
        }

        #[unsafe(method(setBaseWritingDirection:forRange:))]
        fn set_base_writing_direction(
            &self,
            _writing_direction: NSWritingDirection,
            _range: &UITextRange,
        ) {
            // No-op (Phase 1).
        }

        // ---- UITextInput: hit-testing / geometry ----------------------------

        #[unsafe(method(firstRectForRange:))]
        fn first_rect_for_range(&self, range: &UITextRange) -> CGRect {
            self.do_first_rect_for_range(range)
        }

        #[unsafe(method(caretRectForPosition:))]
        fn caret_rect_for_position(&self, position: &UITextPosition) -> CGRect {
            self.do_caret_rect_for_position(position)
        }

        #[unsafe(method_id(selectionRectsForRange:))]
        fn selection_rects_for_range(
            &self,
            _range: &UITextRange,
        ) -> Retained<NSArray<UITextSelectionRect>> {
            NSArray::new()
        }

        #[unsafe(method_id(closestPositionToPoint:))]
        fn closest_position_to_point(
            &self,
            point: bedterm_app::geometry::CGPoint,
        ) -> Option<Retained<UITextPosition>> {
            self.do_closest_position_to_point(point)
        }

        #[unsafe(method_id(closestPositionToPoint:withinRange:))]
        fn closest_position_to_point_within(
            &self,
            point: bedterm_app::geometry::CGPoint,
            range: &UITextRange,
        ) -> Option<Retained<UITextPosition>> {
            self.do_closest_position_to_point_within(point, range)
        }

        #[unsafe(method_id(characterRangeAtPoint:))]
        fn character_range_at_point(&self, point: bedterm_app::geometry::CGPoint) -> Option<Retained<UITextRange>> {
            self.do_character_range_at_point(point)
        }

        // ---- UITextInput: text-in-range / replace ---------------------------

        #[unsafe(method_id(textInRange:))]
        fn text_in_range(&self, range: &UITextRange) -> Option<Retained<NSString>> {
            self.do_text_in_range(range)
        }

        #[unsafe(method(replaceRange:withText:))]
        fn replace_range_with_text(&self, range: &UITextRange, text: &NSString) {
            self.do_replace_range_with_text(range, text)
        }

        // ---- H2 selection long-press selector -------------------------------

        #[unsafe(method(selectionLongPress:))]
        fn selection_long_press(&self, gr: &UIGestureRecognizer) {
            self.do_selection_long_press(gr)
        }

        // ---- H4 pan / scroll selector ---------------------------------------

        #[unsafe(method(scrollPan:))]
        fn scroll_pan(&self, gr: &UIPanGestureRecognizer) {
            self.do_scroll_pan(gr)
        }

        #[unsafe(method(scrollTick:))]
        fn scroll_tick(&self, link: &CADisplayLink) {
            self.do_scroll_tick(link)
        }

        // ---- H5 hardware-key handler ----------------------------------------

        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &NSObject, event: Option<&UIPressesEvent>) {
            self.do_presses_began(presses, event)
        }
    }

    // ---- UIKeyInput conformance ---------------------------------------------
    //
    // Methods MUST live inside this `unsafe impl UIKeyInput` block (not in the
    // class impl above) so objc2's macro-time required-method check sees them
    // and registers protocol conformance with the ObjC runtime.
    unsafe impl UIKeyInput for BtIosMetalInputView {
        #[unsafe(method(hasText))]
        fn has_text(&self) -> bool {
            !self.ivars().text.borrow().is_empty()
        }

        #[unsafe(method(insertText:))]
        fn insert_text(&self, text: &NSString) {
            self.do_insert_text(text)
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            self.do_delete_backward()
        }
    }
);

// Rust trait conformances for parent / sibling protocols. UIKeyInput's
// registration lives inside define_class!; these are no-method shims
// satisfying Rust's trait coherence for UITextInput.
unsafe impl NSObjectProtocol for BtIosMetalInputView {}
unsafe impl UITextInputTraits for BtIosMetalInputView {}
unsafe impl UITextInput for BtIosMetalInputView {}

// Quiet unused-import warnings for items we reference only via `msg_send!`.
#[allow(dead_code)]
fn _force_uses(_: &UIResponder) {
    let _ = objc2::sel!(touchesBegan:withEvent:);
}
