//! UITextInput protocol family bodies for `BtIosMetalInputView`. Covers
//! text/selection, marked text, position arithmetic, tokenizer, hit-testing
//! geometry placeholders, and text replacement.

use super::BtIosMetalInputView;
use crate::geometry::{CGPoint, CGRect, CGSize};
use crate::text_input::{BtIosUITextPosition, BtIosUITextRange};
use objc2::class;
use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2::DefinedClass;
use objc2::MainThreadMarker;
use objc2_foundation::{NSComparisonResult, NSInteger, NSRange, NSString};
use objc2_ui_kit::{UITextLayoutDirection, UITextPosition, UITextRange};

impl BtIosMetalInputView {
    // ---- text / selection --------------------------------------------------

    pub(super) fn do_get_text(&self) -> Retained<NSString> {
        NSString::from_str(&self.ivars().text.borrow())
    }

    pub(super) fn do_get_selected_text_range(&self) -> Option<Retained<UITextRange>> {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let s = BtIosUITextPosition::new(mtm, self.ivars().selected_start.get());
        let e = BtIosUITextPosition::new(mtm, self.ivars().selected_end.get());
        let range = BtIosUITextRange::new(mtm, s, e);
        Some(unsafe { Retained::cast_unchecked::<UITextRange>(range) })
    }

    pub(super) fn do_set_selected_text_range(&self, range: Option<&UITextRange>) {
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

    // ---- marked text -------------------------------------------------------

    pub(super) fn do_get_marked_text_range(&self) -> Option<Retained<UITextRange>> {
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

    pub(super) fn do_set_marked_text(
        &self,
        marked_text: Option<&NSString>,
        selected_range: NSRange,
    ) {
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

    pub(super) fn do_unmark_text(&self) {
        self.ivars().marked_start.set(-1);
        self.ivars().marked_end.set(-1);
    }

    // ---- positions / ranges ------------------------------------------------

    pub(super) fn do_beginning_of_document(&self) -> Retained<UITextPosition> {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, 0))
    }

    pub(super) fn do_end_of_document(&self) -> Retained<UITextPosition> {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let len = self.ivars().text.borrow().len();
        BtIosUITextPosition::into_super(BtIosUITextPosition::new(mtm, len))
    }

    pub(super) fn do_text_range_from(
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

    pub(super) fn do_position_from_offset(
        &self,
        position: &UITextPosition,
        offset: NSInteger,
    ) -> Option<Retained<UITextPosition>> {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let cur = downcast_position(position).index() as i64;
        let len = self.ivars().text.borrow().len() as i64;
        let new = (cur + offset as i64).clamp(0, len) as usize;
        Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(
            mtm, new,
        )))
    }

    pub(super) fn do_position_from_in_direction(
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
        Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(
            mtm, new,
        )))
    }

    pub(super) fn do_compare_position(
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

    pub(super) fn do_offset_from(&self, a: &UITextPosition, b: &UITextPosition) -> NSInteger {
        let ai = downcast_position(a).index() as i64;
        let bi = downcast_position(b).index() as i64;
        (bi - ai) as NSInteger
    }

    // ---- tokenizer ---------------------------------------------------------

    pub(super) fn do_get_tokenizer(&self) -> Retained<NSObject> {
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

    // ---- extent / direction helpers ----------------------------------------

    pub(super) fn do_position_within_range_farthest(
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
        Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(
            mtm, idx,
        )))
    }

    pub(super) fn do_character_range_by_extending(
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

    // ---- hit-testing / geometry --------------------------------------------

    pub(super) fn do_first_rect_for_range(&self, _range: &UITextRange) -> CGRect {
        // Placeholder until glyph rendering lands.
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: 1.0,
                height: 16.0,
            },
        }
    }

    pub(super) fn do_caret_rect_for_position(&self, _position: &UITextPosition) -> CGRect {
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: 1.0,
                height: 16.0,
            },
        }
    }

    pub(super) fn do_closest_position_to_point(
        &self,
        _point: CGPoint,
    ) -> Option<Retained<UITextPosition>> {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let len = self.ivars().text.borrow().len();
        Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(
            mtm, len,
        )))
    }

    pub(super) fn do_closest_position_to_point_within(
        &self,
        _point: CGPoint,
        range: &UITextRange,
    ) -> Option<Retained<UITextPosition>> {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let end = downcast_range(range).end_index();
        Some(BtIosUITextPosition::into_super(BtIosUITextPosition::new(
            mtm, end,
        )))
    }

    pub(super) fn do_character_range_at_point(
        &self,
        _point: CGPoint,
    ) -> Option<Retained<UITextRange>> {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let len = self.ivars().text.borrow().len();
        let s = BtIosUITextPosition::new(mtm, len);
        let e = BtIosUITextPosition::new(mtm, len);
        Some(unsafe { Retained::cast_unchecked::<UITextRange>(BtIosUITextRange::new(mtm, s, e)) })
    }

    // ---- text-in-range / replace -------------------------------------------

    pub(super) fn do_text_in_range(&self, range: &UITextRange) -> Option<Retained<NSString>> {
        let buf = self.ivars().text.borrow();
        let r = downcast_range(range);
        let s = r.start_index().min(buf.len());
        let e = r.end_index().min(buf.len()).max(s);
        // Snap to a UTF-8 boundary so slicing never panics on partial input.
        let s = floor_char_boundary(&buf, s);
        let e = floor_char_boundary(&buf, e);
        Some(NSString::from_str(&buf[s..e]))
    }

    pub(super) fn do_replace_range_with_text(&self, range: &UITextRange, text: &NSString) {
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

    // ---- UIKeyInput bodies (insertText / deleteBackward) -------------------

    pub(super) fn do_insert_text(&self, text: &NSString) {
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
        // Route the inserted text to the PTY sink. When the Ctrl latch
        // is armed and the next byte is an ASCII letter, XOR-mask it to
        // the control-character form (matches Swift `KeyBarState`).
        let bytes = s.as_bytes();
        if !bytes.is_empty() {
            let mut out = bytes.to_vec();
            if self.ivars().ctrl_pending.get() {
                if let Some(b) = out.first_mut() {
                    let lower = b.to_ascii_lowercase();
                    if lower.is_ascii_lowercase() {
                        *b = lower & 0x1F;
                    }
                }
                self.ivars().ctrl_pending.set(false);
                self.notify_ctrl_unlatched();
            }
            self.dispatch_send(&out);
        }
    }

    pub(super) fn do_delete_backward(&self) {
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
        // Backspace → DEL (0x7F). Matches the hardware-key encoder.
        self.dispatch_send(&[0x7F]);
    }
}

// -------- Helpers -----------------------------------------------------------

/// Trust-the-runtime downcast. UIKit only ever hands us positions we vended.
pub(super) fn downcast_position(p: &UITextPosition) -> &BtIosUITextPosition {
    // SAFETY: see contract above.
    unsafe { &*(p as *const UITextPosition as *const BtIosUITextPosition) }
}

pub(super) fn downcast_range(r: &UITextRange) -> &BtIosUITextRange {
    // SAFETY: see contract above.
    unsafe { &*(r as *const UITextRange as *const BtIosUITextRange) }
}

/// Round `idx` down to the nearest UTF-8 character boundary in `s`.
/// `str::is_char_boundary` is stable; `floor_char_boundary` is unstable, so
/// we open-code the equivalent.
pub(super) fn floor_char_boundary(s: &str, idx: usize) -> usize {
    let idx = idx.min(s.len());
    let bytes = s.as_bytes();
    let mut i = idx;
    while i > 0 && (bytes[i] & 0xC0) == 0x80 {
        i -= 1;
    }
    i
}
