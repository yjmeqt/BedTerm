//! IME preedit overlay management for `BtIosMetalInputView`. Owns the
//! `ImePreeditOverlay` ivar lifecycle and positions the overlay relative
//! to the current cursor cell.

use super::BtIosMetalInputView;
use crate::ime_preedit_overlay::ImePreeditOverlay;
use bedterm_app::geometry::{CGPoint, CGRect, CGSize};
use objc2::msg_send;
use objc2::DefinedClass;
use objc2::MainThreadMarker;

impl BtIosMetalInputView {
    pub(crate) fn update_preedit_overlay(&self) {
        let marked_text = {
            let s = self.ivars().marked_start.get();
            let e = self.ivars().marked_end.get();
            if s < 0 || e < 0 || s == e {
                None
            } else {
                let buf = self.ivars().text.borrow();
                let s = (s as usize).min(buf.len());
                let e = (e as usize).min(buf.len()).max(s);
                Some(buf[s..e].to_string())
            }
        };
        let Some(text) = marked_text else {
            self.remove_preedit_overlay();
            return;
        };
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let overlay = {
            let mut slot = self.ivars().preedit_overlay.borrow_mut();
            if let Some(ov) = slot.as_ref() {
                ov.clone()
            } else {
                let ov = ImePreeditOverlay::new(mtm);
                unsafe {
                    let _: () = msg_send![self, addSubview: &*ov];
                }
                *slot = Some(ov.clone());
                ov
            }
        };
        overlay.set_text(&text, 14.0);
        let bounds: CGRect = unsafe { msg_send![self, bounds] };
        let cell = self.ivars().cell_size.get();
        let cur_col = self.ivars().cursor_col.get();
        let cur_row = self.ivars().cursor_row.get();
        let size = overlay.size_that_fits(CGSize {
            width: bounds.size.width,
            height: bounds.size.height,
        });
        let cursor_min_x = f64::from(cur_col) * cell.width;
        let cursor_min_y = f64::from(cur_row) * cell.height;
        let cursor_max_y = cursor_min_y + cell.height;
        let y_below = cursor_max_y + 2.0;
        let origin_y = if y_below + size.height <= bounds.size.height {
            y_below
        } else {
            (cursor_min_y - size.height - 2.0).max(0.0)
        };
        let origin_x = cursor_min_x
            .max(0.0)
            .min((bounds.size.width - size.width).max(0.0));
        let frame = CGRect {
            origin: CGPoint {
                x: origin_x,
                y: origin_y,
            },
            size,
        };
        unsafe {
            let _: () = msg_send![&*overlay, setFrame: frame];
        }
    }

    pub(super) fn remove_preedit_overlay(&self) {
        let mut slot = self.ivars().preedit_overlay.borrow_mut();
        if let Some(ov) = slot.take() {
            unsafe {
                let _: () = msg_send![&*ov, removeFromSuperview];
            }
        }
    }
}
