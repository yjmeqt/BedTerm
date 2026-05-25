//! `BtIosMetalInputView` — an `MTKView` subclass that conforms to `UITextInput`.
//!
//! Phase 1 doesn't render glyphs; it just clears the drawable to a background
//! colour derived from view2's text. All UIKit/Metal interop lives here.
//!
//! Notes on byte vs character handling: we store the text buffer as a UTF-8
//! `String` and use byte indices for the selection. iOS positions are NSString
//! (UTF-16) offsets, so non-ASCII input can show selection jitter under IME.
//! This is a documented Phase 1 limitation (see the design doc).

use crate::color::hash_to_rgba;
use crate::geometry::{CGPoint, CGRect, CGSize};
use crate::text_input::{BtIosUITextPosition, BtIosUITextRange};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{
    class, define_class, extern_class, msg_send, sel, ClassType, DefinedClass, MainThreadMarker,
    MainThreadOnly,
};
use objc2_foundation::{
    NSArray, NSComparisonResult, NSDictionary, NSInteger, NSObjectProtocol, NSRange, NSString,
};
use objc2_metal::MTLDevice;
use objc2_ui_kit::{
    NSWritingDirection, UIEvent, UIKeyInput, UIResponder, UITextInput, UITextInputTraits,
    UITextLayoutDirection, UITextPosition, UITextRange, UITextSelectionRect,
    UITextStorageDirection, UIView,
};
use std::cell::{Cell, RefCell};

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
pub struct Ivars {
    /// Text buffer rendered into NSString on demand. UTF-8.
    text: RefCell<String>,
    /// Byte indices into `text`.
    selected_start: Cell<usize>,
    selected_end: Cell<usize>,
    /// Marked range (active IME composition). `-1` = none.
    marked_start: Cell<i64>,
    marked_end: Cell<i64>,
    /// Weak: UIKit-managed input delegate. Stored as raw pointer (UIKit retains
    /// the delegate itself; we just hand it back when asked).
    input_delegate: Cell<*const AnyObject>,
    /// Lazily initialised `UITextInputStringTokenizer` (caches once built).
    tokenizer: RefCell<Option<Retained<NSObject>>>,
    /// Weak ref to `BtIosKeyboardCoordinator` — set after construction.
    coordinator: Cell<*const AnyObject>,
    /// Weak ref to `ModeState` (Rust raw pointer). Main-thread only.
    /// When set, `canBecomeFirstResponder` returns YES only in State2.
    mode_state: Cell<*const crate::input_mode::ModeState>,
}

// SAFETY: Only accessed on the main thread (MainThreadOnly class).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

// -------- Class -------------------------------------------------------------

define_class!(
    /// `MTKView` subclass that drives view1's clear-color background and
    /// conforms to `UITextInput` so it can host a keyboard.
    //
    // DIAGNOSTIC: temporarily inherit UIView (not MTKView) to test whether
    // MTKView is what's suppressing the keyboard.
    #[unsafe(super(UIView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosMetalInputView"]
    #[ivars = Ivars]
    pub struct BtIosMetalInputView;

    impl BtIosMetalInputView {
        // ---- Init ----------------------------------------------------------

        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(
            this: Allocated<Self>,
            frame: CGRect,
        ) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send![super(this), initWithFrame: frame] }
        }

        // ---- Responder ------------------------------------------------------

        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            let ms = self.ivars().mode_state.get();
            if ms.is_null() {
                true
            } else {
                // SAFETY: caller (VC) keeps the Rc<ModeState> alive for
                // the view's lifetime; this method runs on the main thread.
                let mode = unsafe { (*ms).mode() };
                matches!(mode, crate::input_mode::InputMode::State2)
            }
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &NSObject, event: Option<&UIEvent>) {
            // Notify coordinator so it can transfer focus to us.
            let coord = self.ivars().coordinator.get();
            if !coord.is_null() {
                unsafe {
                    let _: () = msg_send![&*coord, handleView1Tap: self];
                }
            }
            // Forward to super so default behaviour (touch tracking) still runs.
            unsafe {
                let _: () = msg_send![super(self), touchesBegan: touches, withEvent: event];
            }
        }

        // ---- UITextInput: text / selection ----------------------------------

        #[unsafe(method_id(text))]
        fn get_text(&self) -> Retained<NSString> {
            NSString::from_str(&self.ivars().text.borrow())
        }

        #[unsafe(method_id(selectedTextRange))]
        fn get_selected_text_range(&self) -> Option<Retained<UITextRange>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let s = BtIosUITextPosition::new(mtm, self.ivars().selected_start.get());
            let e = BtIosUITextPosition::new(mtm, self.ivars().selected_end.get());
            let range = BtIosUITextRange::new(mtm, s, e);
            // Upcast.
            Some(unsafe { Retained::cast_unchecked::<UITextRange>(range) })
        }

        #[unsafe(method(setSelectedTextRange:))]
        fn set_selected_text_range(&self, range: Option<&UITextRange>) {
            match range {
                Some(r) => {
                    // SAFETY: we only ever vend BtIosUITextRange instances to
                    // UIKit, so anything coming back is one of ours.
                    let r: &BtIosUITextRange =
                        unsafe { &*(r as *const UITextRange as *const BtIosUITextRange) };
                    let len = self.ivars().text.borrow().len();
                    self.ivars().selected_start.set(r.start_index().min(len));
                    self.ivars().selected_end.set(r.end_index().min(len));
                }
                None => {
                    let len = self.ivars().text.borrow().len();
                    self.ivars().selected_start.set(len);
                    self.ivars().selected_end.set(len);
                }
            }
        }

        // ---- UITextInput: marked text ---------------------------------------

        #[unsafe(method_id(markedTextRange))]
        fn get_marked_text_range(&self) -> Option<Retained<UITextRange>> {
            let s = self.ivars().marked_start.get();
            let e = self.ivars().marked_end.get();
            if s < 0 || e < 0 {
                None
            } else {
                let mtm = unsafe { MainThreadMarker::new_unchecked() };
                let sp = BtIosUITextPosition::new(mtm, s as usize);
                let ep = BtIosUITextPosition::new(mtm, e as usize);
                let r = BtIosUITextRange::new(mtm, sp, ep);
                Some(unsafe { Retained::cast_unchecked::<UITextRange>(r) })
            }
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
            let new_str = marked_text.map(|s| s.to_string()).unwrap_or_default();

            // The replacement target is either the existing marked range or
            // the current selection.
            let mut buf = self.ivars().text.borrow_mut();
            let (rep_start, rep_end) = {
                let ms = self.ivars().marked_start.get();
                let me = self.ivars().marked_end.get();
                if ms >= 0 && me >= 0 {
                    (ms as usize, me as usize)
                } else {
                    (
                        self.ivars().selected_start.get(),
                        self.ivars().selected_end.get(),
                    )
                }
            };
            let rep_start = rep_start.min(buf.len());
            let rep_end = rep_end.min(buf.len()).max(rep_start);

            buf.replace_range(rep_start..rep_end, &new_str);
            let new_marked_end = rep_start + new_str.len();
            if new_str.is_empty() {
                self.ivars().marked_start.set(-1);
                self.ivars().marked_end.set(-1);
            } else {
                self.ivars().marked_start.set(rep_start as i64);
                self.ivars().marked_end.set(new_marked_end as i64);
            }

            // selectedRange is relative to the marked text (per Apple docs).
            // We treat it as byte offsets; for ASCII this is correct.
            let sel_start = rep_start + selected_range.location.min(new_str.len());
            let sel_end = (sel_start + selected_range.length).min(new_marked_end);
            self.ivars().selected_start.set(sel_start);
            self.ivars().selected_end.set(sel_end);
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            self.ivars().marked_start.set(-1);
            self.ivars().marked_end.set(-1);
        }

        // ---- UITextInput: positions / ranges --------------------------------

        #[unsafe(method_id(beginningOfDocument))]
        fn beginning_of_document(&self) -> Retained<UITextPosition> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, 0))
        }

        #[unsafe(method_id(endOfDocument))]
        fn end_of_document(&self) -> Retained<UITextPosition> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let len = self.ivars().text.borrow().len();
            BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, len))
        }

        #[unsafe(method_id(textRangeFromPosition:toPosition:))]
        fn text_range_from(
            &self,
            from: &UITextPosition,
            to: &UITextPosition,
        ) -> Option<Retained<UITextRange>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let a = downcast_position(from).index();
            let b = downcast_position(to).index();
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let s = BtIosUITextPosition::new(mtm, lo);
            let e = BtIosUITextPosition::new(mtm, hi);
            Some(unsafe { Retained::cast_unchecked::<UITextRange>(BtIosUITextRange::new(mtm, s, e)) })
        }

        #[unsafe(method_id(positionFromPosition:offset:))]
        fn position_from_offset(
            &self,
            position: &UITextPosition,
            offset: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let cur = downcast_position(position).index() as i64;
            let len = self.ivars().text.borrow().len() as i64;
            let new = (cur + offset as i64).clamp(0, len) as usize;
            Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, new)))
        }

        #[unsafe(method_id(positionFromPosition:inDirection:offset:))]
        fn position_from_in_direction(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
            offset: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let cur = downcast_position(position).index() as i64;
            let len = self.ivars().text.borrow().len() as i64;
            // Phase 1 is single-line; up/down jump to bounds.
            let new = match direction {
                UITextLayoutDirection::Right => (cur + offset as i64).clamp(0, len),
                UITextLayoutDirection::Left => (cur - offset as i64).clamp(0, len),
                UITextLayoutDirection::Up => 0,
                UITextLayoutDirection::Down => len,
                _ => cur,
            } as usize;
            Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, new)))
        }

        #[unsafe(method(comparePosition:toPosition:))]
        fn compare_position(
            &self,
            a: &UITextPosition,
            b: &UITextPosition,
        ) -> NSComparisonResult {
            let ai = downcast_position(a).index();
            let bi = downcast_position(b).index();
            if ai < bi {
                NSComparisonResult::Ascending
            } else if ai > bi {
                NSComparisonResult::Descending
            } else {
                NSComparisonResult::Same
            }
        }

        #[unsafe(method(offsetFromPosition:toPosition:))]
        fn offset_from(&self, a: &UITextPosition, b: &UITextPosition) -> NSInteger {
            let ai = downcast_position(a).index() as i64;
            let bi = downcast_position(b).index() as i64;
            (bi - ai) as NSInteger
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
            let cached = self.ivars().tokenizer.borrow().clone();
            if let Some(t) = cached {
                t
            } else {
                // Lazy init UITextInputStringTokenizer.
                let cls = class!(UITextInputStringTokenizer);
                let tok: Retained<NSObject> = unsafe {
                    let alloc: Allocated<NSObject> = msg_send![cls, alloc];
                    msg_send![
                        alloc,
                        initWithTextInput: self as *const _ as *const AnyObject
                    ]
                };
                *self.ivars().tokenizer.borrow_mut() = Some(tok.clone());
                tok
            }
        }

        // ---- UITextInput: extent / direction helpers ------------------------

        #[unsafe(method_id(positionWithinRange:farthestInDirection:))]
        fn position_within_range_farthest(
            &self,
            range: &UITextRange,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextPosition>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let r = downcast_range(range);
            let idx = match direction {
                UITextLayoutDirection::Left | UITextLayoutDirection::Up => r.start_index(),
                _ => r.end_index(),
            };
            Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, idx)))
        }

        #[unsafe(method_id(characterRangeByExtendingPosition:inDirection:))]
        fn character_range_by_extending(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextRange>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let cur = downcast_position(position).index();
            let len = self.ivars().text.borrow().len();
            let (lo, hi) = match direction {
                UITextLayoutDirection::Left | UITextLayoutDirection::Up => (0, cur),
                _ => (cur, len),
            };
            let s = BtIosUITextPosition::new(mtm, lo);
            let e = BtIosUITextPosition::new(mtm, hi);
            Some(unsafe { Retained::cast_unchecked::<UITextRange>(BtIosUITextRange::new(mtm, s, e)) })
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
        fn first_rect_for_range(&self, _range: &UITextRange) -> CGRect {
            // Placeholder until glyph rendering lands.
            CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize { width: 1.0, height: 16.0 },
            }
        }

        #[unsafe(method(caretRectForPosition:))]
        fn caret_rect_for_position(&self, _position: &UITextPosition) -> CGRect {
            CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize { width: 1.0, height: 16.0 },
            }
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
            _point: CGPoint,
        ) -> Option<Retained<UITextPosition>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let len = self.ivars().text.borrow().len();
            Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, len)))
        }

        #[unsafe(method_id(closestPositionToPoint:withinRange:))]
        fn closest_position_to_point_within(
            &self,
            _point: CGPoint,
            range: &UITextRange,
        ) -> Option<Retained<UITextPosition>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let end = downcast_range(range).end_index();
            Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, end)))
        }

        #[unsafe(method_id(characterRangeAtPoint:))]
        fn character_range_at_point(&self, _point: CGPoint) -> Option<Retained<UITextRange>> {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let len = self.ivars().text.borrow().len();
            let s = BtIosUITextPosition::new(mtm, len);
            let e = BtIosUITextPosition::new(mtm, len);
            Some(unsafe { Retained::cast_unchecked::<UITextRange>(BtIosUITextRange::new(mtm, s, e)) })
        }

        // ---- UITextInput: text-in-range / replace ---------------------------

        #[unsafe(method_id(textInRange:))]
        fn text_in_range(&self, range: &UITextRange) -> Option<Retained<NSString>> {
            let buf = self.ivars().text.borrow();
            let r = downcast_range(range);
            let s = r.start_index().min(buf.len());
            let e = r.end_index().min(buf.len()).max(s);
            // Snap to a UTF-8 boundary so slicing never panics on partial input.
            let s = floor_char_boundary(&buf, s);
            let e = floor_char_boundary(&buf, e);
            Some(NSString::from_str(&buf[s..e]))
        }

        #[unsafe(method(replaceRange:withText:))]
        fn replace_range_with_text(&self, range: &UITextRange, text: &NSString) {
            let new_str = text.to_string();
            let mut buf = self.ivars().text.borrow_mut();
            let r = downcast_range(range);
            let s = floor_char_boundary(&buf, r.start_index().min(buf.len()));
            let e = floor_char_boundary(&buf, r.end_index().min(buf.len()).max(s));
            buf.replace_range(s..e, &new_str);
            let new_caret = s + new_str.len();
            self.ivars().selected_start.set(new_caret);
            self.ivars().selected_end.set(new_caret);
            self.ivars().marked_start.set(-1);
            self.ivars().marked_end.set(-1);
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
            let s = text.to_string();
            {
                let mut buf = self.ivars().text.borrow_mut();
                let start = self.ivars().selected_start.get().min(buf.len());
                let end = self.ivars().selected_end.get().min(buf.len()).max(start);
                buf.replace_range(start..end, &s);
                let new_caret = start + s.len();
                self.ivars().selected_start.set(new_caret);
                self.ivars().selected_end.set(new_caret);
                self.ivars().marked_start.set(-1);
                self.ivars().marked_end.set(-1);
            }
            self.refresh_bg_from_text();
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            {
                let mut buf = self.ivars().text.borrow_mut();
                let start = self.ivars().selected_start.get().min(buf.len());
                let end = self.ivars().selected_end.get().min(buf.len()).max(start);
                if start != end {
                    buf.replace_range(start..end, "");
                    self.ivars().selected_start.set(start);
                    self.ivars().selected_end.set(start);
                } else if start > 0 {
                    let prev = buf[..start]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    buf.replace_range(prev..start, "");
                    self.ivars().selected_start.set(prev);
                    self.ivars().selected_end.set(prev);
                }
            }
            self.refresh_bg_from_text();
        }
    }
);

// Rust trait conformances for parent / sibling protocols. UIKeyInput's
// registration lives inside define_class!; these are no-method shims
// satisfying Rust's trait coherence for UITextInput.
unsafe impl NSObjectProtocol for BtIosMetalInputView {}
unsafe impl UITextInputTraits for BtIosMetalInputView {}
unsafe impl UITextInput for BtIosMetalInputView {}

// -------- Public Rust-side helpers ------------------------------------------

impl BtIosMetalInputView {
    /// Build a view bound to a Metal device. The caller retains ownership.
    /// Returns `None` if MTKView's designated initialiser bails (rare; happens
    /// in some headless test contexts).
    pub fn new(
        mtm: MainThreadMarker,
        _device: &ProtocolObject<dyn MTLDevice>,
    ) -> Option<Retained<Self>> {
        let zero = CGRect::default();
        let this: Option<Retained<Self>> =
            unsafe { msg_send![Self::alloc(mtm), initWithFrame: zero] };
        if let Some(ref v) = this {
            unsafe {
                let _: () = msg_send![&**v, setUserInteractionEnabled: true];
            }
        }
        this
    }

    /// Update the background colour. RGBA components are 0..1.
    ///
    /// Phase 1 paints via UIView's `backgroundColor` with the Metal layer set
    /// non-opaque — the MTKView is paused, doesn't draw, and the UIView fill
    /// shows through. Phase 2 (glyph rendering) will switch this to a real
    /// `setClearColor:` once we have a working Metal pipeline that can clear +
    /// present a frame. Passing `MTLClearColor` by value through `msg_send!` in
    /// objc2 0.5 mis-marshals the 32-byte struct on arm64 (crash).
    pub fn set_bg_color(&self, rgba: (f32, f32, f32, f32)) {
        let color: Retained<objc2_ui_kit::UIColor> = unsafe {
            msg_send![
                objc2_ui_kit::UIColor::class(),
                colorWithRed: rgba.0 as f64,
                green: rgba.1 as f64,
                blue: rgba.2 as f64,
                alpha: rgba.3 as f64,
            ]
        };
        unsafe {
            let _: () = msg_send![self, setBackgroundColor: &*color];
        }
    }

    /// Install the keyboard coordinator (weak pointer; coordinator owns its
    /// own lifetime and is reachable for the view's lifetime in practice).
    pub fn set_coordinator(&self, coordinator: *const AnyObject) {
        self.ivars().coordinator.set(coordinator);
    }

    /// Install the shared `ModeState` (raw pointer; the VC keeps the
    /// `Rc<ModeState>` alive for the view's lifetime).
    pub fn set_mode_state(&self, ms: *const crate::input_mode::ModeState) {
        self.ivars().mode_state.set(ms);
    }

    /// Recompute the background colour from the current text buffer.
    /// Called after every insertText:/deleteBackward:.
    fn refresh_bg_from_text(&self) {
        let rgba = hash_to_rgba(&self.ivars().text.borrow());
        self.set_bg_color(rgba);
    }
}

// -------- Helpers -----------------------------------------------------------

/// Trust-the-runtime downcast. UIKit only ever hands us positions we vended.
fn downcast_position(p: &UITextPosition) -> &BtIosUITextPosition {
    // SAFETY: see contract above.
    unsafe { &*(p as *const UITextPosition as *const BtIosUITextPosition) }
}

fn downcast_range(r: &UITextRange) -> &BtIosUITextRange {
    // SAFETY: see contract above.
    unsafe { &*(r as *const UITextRange as *const BtIosUITextRange) }
}

/// Round `idx` down to the nearest UTF-8 character boundary in `s`.
/// `str::is_char_boundary` is stable; `floor_char_boundary` is unstable, so
/// we open-code the equivalent.
fn floor_char_boundary(s: &str, idx: usize) -> usize {
    let idx = idx.min(s.len());
    let bytes = s.as_bytes();
    let mut i = idx;
    while i > 0 && (bytes[i] & 0xC0) == 0x80 {
        i -= 1;
    }
    i
}

// Quiet unused-import warnings for items we reference only via `msg_send!`.
#[allow(dead_code)]
fn _force_uses(_: &UIResponder) {
    let _ = sel!(touchesBegan:withEvent:);
}
