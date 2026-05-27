//! Layout, draw, responder, and touch-began plumbing for
//! `BtIosMetalInputView`. The methods here are the bodies of the
//! corresponding selectors in `mod.rs`.

use super::BtIosMetalInputView;
use crate::geometry::CGRect;
use bedterm_core::ffi::{bt_term_resize, BtTerm};
use objc2::msg_send;
use objc2::runtime::{AnyObject, NSObject};
use objc2::sel;
use objc2::DefinedClass;
use objc2_ui_kit::UIEvent;

impl BtIosMetalInputView {
    pub(super) fn do_can_become_first_responder(&self) -> bool {
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

    pub(super) fn do_layout_subviews(&self) {
        unsafe {
            let _: () = msg_send![super(self), layoutSubviews];
        }
        // Recompute cols/rows from the renderer's cell-pixel-size and
        // the view's pixel bounds (scaled by contentScaleFactor).
        let (cw, ch) = self.ivars().cell_px.get();
        if cw == 0 || ch == 0 {
            return;
        }
        let bounds: CGRect = unsafe { msg_send![self, bounds] };
        let scale: f64 = unsafe { msg_send![self, contentScaleFactor] };
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let px_w = (bounds.size.width * scale).max(0.0);
        let px_h = (bounds.size.height * scale).max(0.0);
        let cols = ((px_w / f64::from(cw)).floor() as u16).max(1);
        let rows = ((px_h / f64::from(ch)).floor() as u16).max(1);
        let prev = self.ivars().grid_dim.get();
        if prev != (cols, rows) {
            let term = self.ivars().term.get();
            if !term.is_null() {
                unsafe { bt_term_resize(term, cols, rows) };
            }
            self.ivars().grid_dim.set((cols, rows));
            // Notify the Swift host so it can resize the SSH PTY to
            // match the actual rendered grid — otherwise the shell
            // formats for whatever cols/rows the session was opened
            // with (default 80×24) and wraps oddly.
            if let Some(cb) = self.ivars().on_resize.borrow().as_ref() {
                cb(cols, rows);
            }
        }
    }

    pub(super) fn do_draw_rect(&self, _rect: CGRect) {
        // Skip when the renderer didn't initialise (headless tests).
        let mut slot = self.ivars().renderer.borrow_mut();
        let Some(renderer) = slot.as_mut() else {
            return;
        };
        // Probe MTKView accessors. The view temporarily inherits UIView
        // (diagnostic mode — see the `define_class!` comment) so these
        // selectors may be unknown; bail safely if so. Once MTKView
        // inheritance is restored these will resolve to the real
        // drawable / RPD and the pass below will paint glyphs.
        let responds_drawable: bool =
            unsafe { msg_send![self, respondsToSelector: sel!(currentDrawable)] };
        if !responds_drawable {
            return;
        }
        let drawable: *mut AnyObject = unsafe { msg_send![self, currentDrawable] };
        if drawable.is_null() {
            return;
        }
        let texture: *mut AnyObject = unsafe { msg_send![&*drawable, texture] };
        if texture.is_null() {
            return;
        }
        let bounds: CGRect = unsafe { msg_send![self, bounds] };
        let scale: f64 = unsafe { msg_send![self, contentScaleFactor] };
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let vp_w = (bounds.size.width * scale).max(0.0) as u32;
        let vp_h = (bounds.size.height * scale).max(0.0) as u32;
        let term = self.ivars().term.get();
        let _ = unsafe {
            renderer.draw(
                term as *const BtTerm,
                texture as *const std::ffi::c_void,
                vp_w,
                vp_h,
                0.0,
            )
        };
        unsafe {
            let _: () = msg_send![&*drawable, present];
        }
    }

    pub(super) fn do_touches_began(&self, touches: &NSObject, event: Option<&UIEvent>) {
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
}
