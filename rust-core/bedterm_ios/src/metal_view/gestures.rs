//! Selection long-press + pan/scroll gesture bodies, plus the scroll
//! inertia state machine driven by a `CADisplayLink`.

use super::BtIosMetalInputView;
use crate::geometry::CGPoint;
use crate::metal_selection_layer::SelectionRange;
use crate::scroll_physics::ScrollPhysics;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::DefinedClass;
use objc2::{sel, MainThreadMarker};
use objc2_quartz_core::CADisplayLink;
use objc2_ui_kit::{UIGestureRecognizer, UIGestureRecognizerState, UIPanGestureRecognizer};

impl BtIosMetalInputView {
    // ---- H2 selection long-press ------------------------------------------

    pub(super) fn do_selection_long_press(&self, gr: &UIGestureRecognizer) {
        let cell = self.ivars().cell_size.get();
        if cell.width <= 0.0 || cell.height <= 0.0 {
            return;
        }
        let pt: CGPoint = unsafe { msg_send![gr, locationInView: self] };
        let col = (pt.x / cell.width).max(0.0) as i32;
        let row = (pt.y / cell.height).max(0.0) as i32;
        let state: UIGestureRecognizerState = unsafe { msg_send![gr, state] };
        match state {
            UIGestureRecognizerState::Began => {
                let r = SelectionRange {
                    start_row: row,
                    start_col: col,
                    end_row: row,
                    end_col: col + 1,
                };
                self.ivars().selection_range.set(Some(r));
            }
            UIGestureRecognizerState::Changed => {
                if let Some(mut cur) = self.ivars().selection_range.get() {
                    cur.end_row = row;
                    cur.end_col = col + 1;
                    self.ivars().selection_range.set(Some(cur));
                }
            }
            UIGestureRecognizerState::Ended => {
                // Copy hook lands in W7; for now keep the selection visible
                // until the next gesture clears it.
            }
            UIGestureRecognizerState::Cancelled | UIGestureRecognizerState::Failed => {
                self.ivars().selection_range.set(None);
            }
            _ => {}
        }
        self.refresh_selection_layer();
    }

    // ---- H4 pan / scroll --------------------------------------------------

    pub(super) fn do_scroll_pan(&self, gr: &UIPanGestureRecognizer) {
        let state: UIGestureRecognizerState = unsafe { msg_send![gr, state] };
        match state {
            UIGestureRecognizerState::Began => {
                self.stop_inertia();
                self.ivars().drag_accumulator.set(0.0);
            }
            UIGestureRecognizerState::Changed => {
                let t: CGPoint = unsafe { msg_send![gr, translationInView: self] };
                let zero = CGPoint { x: 0.0, y: 0.0 };
                unsafe {
                    let _: () = msg_send![gr, setTranslation: zero, inView: self];
                }
                self.apply_scroll(t.y);
            }
            UIGestureRecognizerState::Ended | UIGestureRecognizerState::Cancelled => {
                let v: CGPoint = unsafe { msg_send![gr, velocityInView: self] };
                if v.y.abs() > 50.0 {
                    self.start_inertia(v.y);
                }
            }
            _ => {}
        }
    }

    pub(super) fn do_scroll_tick(&self, _link: &CADisplayLink) {
        // Use CACurrentMediaTime via the system clock; we don't need
        // perfect monotonicity, just a steady delta.
        let now = current_media_time();
        let pts = {
            let mut p_ref = self.ivars().scroll_physics.borrow_mut();
            let Some(p) = p_ref.as_mut() else {
                return;
            };
            let pts = p.step(now);
            let exhausted = p.velocity.abs() < 30.0;
            if exhausted {
                *p_ref = None;
                drop(p_ref);
                self.stop_inertia();
                return;
            }
            pts
        };
        self.apply_scroll(pts);
    }

    // ---- inertia helpers --------------------------------------------------

    pub(super) fn apply_scroll(&self, points: f64) {
        let cell = self.ivars().cell_size.get();
        if cell.height <= 0.0 {
            return;
        }
        let mut acc = self.ivars().drag_accumulator.get() + points;
        let rows = (acc / cell.height).trunc() as i32;
        if rows != 0 {
            acc -= f64::from(rows) * cell.height;
            self.ivars().pending_scroll_rows.set(rows);
        }
        self.ivars().drag_accumulator.set(acc);
    }

    /// Number of whole-row scrolls accumulated since the last reader call.
    /// W6/W7 reads this from a CADisplayLink in the host VC to drive the
    /// real `TerminalCore::scrollBy`.
    #[allow(dead_code)]
    pub fn drain_pending_scroll(&self) -> i32 {
        let r = self.ivars().pending_scroll_rows.get();
        self.ivars().pending_scroll_rows.set(0);
        r
    }

    pub(super) fn start_inertia(&self, initial_velocity: f64) {
        self.stop_inertia();
        // Aggressive decay matching Swift parity (`terminalScrollPhysics`).
        let mut p = ScrollPhysics::new(0.001, 1.0);
        p.velocity = initial_velocity;
        p.last_tick = current_media_time();
        *self.ivars().scroll_physics.borrow_mut() = Some(p);
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let _ = mtm;
        let link: Retained<CADisplayLink> =
            unsafe { CADisplayLink::displayLinkWithTarget_selector(self, sel!(scrollTick:)) };
        unsafe {
            let runloop = objc2_foundation::NSRunLoop::mainRunLoop();
            link.addToRunLoop_forMode(&runloop, objc2_foundation::NSRunLoopCommonModes);
        }
        *self.ivars().display_link.borrow_mut() = Some(link);
    }

    pub fn stop_inertia(&self) {
        if let Some(link) = self.ivars().display_link.borrow_mut().take() {
            link.invalidate();
        }
        *self.ivars().scroll_physics.borrow_mut() = None;
    }
}

/// `CACurrentMediaTime()` via direct C-extern. Avoids pulling the entire
/// QuartzCore typed binding for a single function. Matches what `debug_hud`
/// would get from the `CADisplayLink.timestamp` accessor; the two clocks are
/// the same.
pub(super) fn current_media_time() -> f64 {
    extern "C" {
        fn CACurrentMediaTime() -> f64;
    }
    unsafe { CACurrentMediaTime() }
}
