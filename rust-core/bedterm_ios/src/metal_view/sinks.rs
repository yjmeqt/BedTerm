//! Byte-emission and resize sinks for `BtIosMetalInputView`, plus the
//! Rust-side helpers that drive the existing on-screen-keyboard overlays
//! (cursor / selection layers), Ctrl-latch state, and replay path.

use super::BtIosMetalInputView;
use crate::color::hash_to_rgba;
use crate::geometry::CGSize;
use crate::metal_cursor_layer::MetalCursorLayer;
use crate::metal_selection_layer::{MetalSelectionLayer, SelectionRange};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{sel, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_ui_kit::{UILongPressGestureRecognizer, UIPanGestureRecognizer};

/// Boxed resize sink. Installed by `BtIosMetalInputView::set_on_resize`.
pub type OnResizeSink = Box<dyn Fn(u16, u16)>;

/// Boxed byte-emission sink. Installed by `BtIosMetalInputView::set_on_send`.
pub type OnSendSink = Box<dyn Fn(&[u8])>;

// -------- Public Rust-side helpers ------------------------------------------

impl BtIosMetalInputView {
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
    pub(super) fn refresh_bg_from_text(&self) {
        let rgba = hash_to_rgba(&self.ivars().text.borrow());
        self.set_bg_color(rgba);
    }
}

// -------- H1/H2/H3/H4/H5 Rust-side API ---------------------------------
//
// These methods land in W6/W7 of the input-composer port; they are exposed
// now so the layer / gesture / inertia / replay / hardware-key plumbing is
// ready when the host VC wiring drops in. Dead-code suppressed at the impl
// boundary so clippy `-D warnings` stays green until the consumers land.
#[allow(dead_code)]
impl BtIosMetalInputView {
    /// Install the cursor + selection sublayers and the long-press / pan
    /// recognisers. Idempotent. Call once from the host VC (W7) before the
    /// view is shown; for now it is a no-op until the host opts in so that
    /// the existing tests aren't perturbed.
    pub fn install_metal_overlays(&self) {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let _ = mtm;
        // Cursor.
        if self.ivars().cursor_layer.borrow().is_none() {
            let c = MetalCursorLayer::new();
            unsafe {
                let host_layer: Retained<AnyObject> = msg_send![self, layer];
                let _: () = msg_send![&*host_layer, addSublayer: c.layer()];
            }
            *self.ivars().cursor_layer.borrow_mut() = Some(c);
        }
        // Selection.
        if self.ivars().selection_layer.borrow().is_none() {
            let s = MetalSelectionLayer::new();
            unsafe {
                let host_layer: Retained<AnyObject> = msg_send![self, layer];
                let _: () = msg_send![&*host_layer, addSublayer: s.layer()];
            }
            *self.ivars().selection_layer.borrow_mut() = Some(s);
        }
        // Long-press selection gesture.
        if self.ivars().selection_gr.borrow().is_none() {
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
            unsafe {
                let _: () = msg_send![self, addGestureRecognizer: &*lp];
            }
            *self.ivars().selection_gr.borrow_mut() = Some(lp);
        }
        // Pan gesture.
        if self.ivars().pan_gr.borrow().is_none() {
            let pan: Retained<UIPanGestureRecognizer> = unsafe {
                let alloc = UIPanGestureRecognizer::alloc(mtm);
                let target: *const AnyObject = self as *const Self as *const AnyObject;
                msg_send![
                    alloc,
                    initWithTarget: target,
                    action: sel!(scrollPan:),
                ]
            };
            pan.setMaximumNumberOfTouches(1);
            unsafe {
                let _: () = msg_send![self, addGestureRecognizer: &*pan];
            }
            *self.ivars().pan_gr.borrow_mut() = Some(pan);
        }
    }

    /// Tell the view about a new cell size + grid dimensions. Drives
    /// cursor / selection geometry.
    pub fn set_geometry(&self, cell_size: CGSize, cols: i32, rows: i32) {
        self.ivars().cell_size.set(cell_size);
        self.ivars().cols.set(cols);
        self.ivars().rows.set(rows);
        if let Some(c) = self.ivars().cursor_layer.borrow().as_ref() {
            let col = self.ivars().cursor_col.get();
            let row = self.ivars().cursor_row.get();
            c.update(col, row, cell_size);
        }
        self.refresh_selection_layer();
    }

    /// Move the cursor (in cells). Hidden when `row < 0`.
    pub fn set_cursor(&self, col: i32, row: i32, visible: bool) {
        self.ivars().cursor_col.set(col);
        self.ivars().cursor_row.set(row);
        if let Some(c) = self.ivars().cursor_layer.borrow().as_ref() {
            c.set_hidden(!visible);
            if visible {
                c.update(col, row, self.ivars().cell_size.get());
            }
        }
        self.update_preedit_overlay();
    }

    /// Update the selection overlay from the current `selection_range`.
    pub(crate) fn refresh_selection_layer(&self) {
        let Some(layer) = self
            .ivars()
            .selection_layer
            .borrow()
            .as_ref()
            .map(|s| s as *const _)
        else {
            return;
        };
        // SAFETY: layer pointer borrowed from the same RefCell guard above,
        // we deref it back via the borrow taken next.
        let _ = layer;
        let cell = self.ivars().cell_size.get();
        let cols = self.ivars().cols.get();
        let sel = self.ivars().selection_range.get();
        if let Some(s) = self.ivars().selection_layer.borrow().as_ref() {
            s.update(sel, cell, cols);
        }
    }

    /// Set the current selection from outside (e.g. tests or W7 wiring).
    #[allow(dead_code)]
    pub fn set_selection(&self, range: Option<SelectionRange>) {
        self.ivars().selection_range.set(range);
        self.refresh_selection_layer();
    }

    /// Install the byte-emission sink. Called by the VC (W6/W7) to route
    /// hardware-key + IME-commit bytes into the live PTY.
    pub fn set_on_send(&self, on_send: OnSendSink) {
        *self.ivars().on_send.borrow_mut() = Some(on_send);
    }

    /// Install (or clear) the resize-notification callback. Fired from
    /// `layoutSubviews` whenever the renderer-derived (cols, rows) changes
    /// — used by the Swift host to keep the SSH PTY in step with the
    /// actual rendered grid.
    pub fn set_on_resize(&self, cb: Option<OnResizeSink>) {
        *self.ivars().on_resize.borrow_mut() = cb;
    }

    /// Current (cols, rows) most recently reported to / cached by the
    /// terminal. Returns `(0, 0)` before the first layout pass.
    pub fn grid_dim(&self) -> (u16, u16) {
        self.ivars().grid_dim.get()
    }

    /// Arm / disarm the Ctrl latch. When armed, the next ASCII-letter byte
    /// emitted via `insert_text` (on-screen keyboard) is XOR-masked to its
    /// control-character form and the latch auto-clears.
    pub fn set_ctrl_pending(&self, pending: bool) {
        self.ivars().ctrl_pending.set(pending);
    }

    /// Returns whether the Ctrl latch is currently armed.
    pub fn ctrl_pending(&self) -> bool {
        self.ivars().ctrl_pending.get()
    }

    /// Emit a raw byte slice through the installed `on_send` sink — the
    /// chip-emission path used by the coordinator (Esc, Tab, dpad, send).
    /// Mirrors `dispatch_send` but exposed publicly for the coordinator.
    pub fn emit(&self, bytes: &[u8]) {
        self.dispatch_send(bytes);
    }

    /// Notify the coordinator (when installed) that the latch was just
    /// consumed by an in-view emission, so it can clear its visual tint.
    pub(super) fn notify_ctrl_unlatched(&self) {
        let coord = self.ivars().coordinator.get();
        if coord.is_null() {
            return;
        }
        unsafe {
            let _: () = msg_send![&*coord, ctrlLatchConsumed];
        }
    }

    /// Replay a slice of bytes into the view. In Phase 1 this only forwards
    /// the bytes through `on_send` (mirroring the Swift implementation's
    /// outbound replay channel). Real grid replay lives behind the W6/W7
    /// session wiring.
    pub fn replay_bytes(&self, bytes: &[u8]) {
        self.dispatch_send(bytes);
    }

    pub(crate) fn dispatch_send(&self, bytes: &[u8]) {
        if let Some(sink) = self.ivars().on_send.borrow().as_ref() {
            sink(bytes);
        }
    }
}
