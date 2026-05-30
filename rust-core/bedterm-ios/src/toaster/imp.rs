//! iOS-only `BtIosToasterView` — the live toast container view.

#![cfg(target_os = "ios")]

use super::card::{make_toast_card, ToastCardViews};
use super::{
    offset_for, opacity_for, scale_for, should_autodismiss, ToastAction, ToastKind, MAX_VISIBLE,
};
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::encode::{Encode, Encoding, RefEncode};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_ui_kit::{UIStackView, UIView};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

/// `CGAffineTransform` — `[a b; c d]` linear part plus `(tx, ty)`
/// translation. Mirrors the `#[repr(C)]` + objc2 `Encode` pattern used by
/// the geometry primitives so it can cross `setTransform:`.
#[repr(C)]
#[derive(Clone, Copy)]
struct CGAffineTransform {
    a: CGFloat,
    b: CGFloat,
    c: CGFloat,
    d: CGFloat,
    tx: CGFloat,
    ty: CGFloat,
}

impl CGAffineTransform {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Uniform scale `s` plus a vertical translate of `ty` points.
    fn scale_translate_y(s: CGFloat, ty: CGFloat) -> Self {
        Self {
            a: s,
            b: 0.0,
            c: 0.0,
            d: s,
            tx: 0.0,
            ty,
        }
    }
}

// SAFETY: `#[repr(C)]` with the documented CGAffineTransform field layout.
unsafe impl Encode for CGAffineTransform {
    const ENCODING: Encoding = Encoding::Struct(
        "CGAffineTransform",
        &[
            CGFloat::ENCODING,
            CGFloat::ENCODING,
            CGFloat::ENCODING,
            CGFloat::ENCODING,
            CGFloat::ENCODING,
            CGFloat::ENCODING,
        ],
    );
}
unsafe impl RefEncode for CGAffineTransform {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
}

/// Animation duration for stack/insert/dismiss transitions (s).
const ANIM_DURATION: f64 = 0.35;
/// `UIViewAnimationOptionCurveEaseOut` (2 << 16 = 0x2_0000)
/// | `BeginFromCurrentState` (1 << 2 = 0x4)
/// | `AllowUserInteraction` (1 << 1 = 0x2). Written as a single literal
/// to avoid a clippy `eq_op` false positive on the shift operands.
const ANIM_OPTIONS: u64 = 0x2_0006;

/// Outer horizontal inset of each card from the container edges (pt).
const H_MARGIN: CGFloat = 16.0;
/// Inset of a card's content stack from the card edges (pt).
pub(super) const CARD_PAD: CGFloat = 12.0;
/// Top inset of the frontmost card from the container's top edge (pt).
const TOP_PAD: CGFloat = 8.0;

/// `UIGestureRecognizerStateEnded`.
const GESTURE_STATE_ENDED: i64 = 3;
/// Swipe-up distance (pt) past which a non-persistent toast dismisses.
const SWIPE_DISMISS_THRESHOLD: CGFloat = -30.0;
/// Distance (pt) the incoming front card slides down from on insert.
const INSERT_SLIDE: CGFloat = 16.0;
/// Scale the front card shrinks to as it fades out on dismiss.
const DISMISS_SCALE: CGFloat = 0.9;

/// C callback fired when a toast action button is tapped: `(ctx, id, index)`.
type OnAction = unsafe extern "C" fn(ctx: *mut c_void, id: u64, action_index: u32);
/// C callback fired when a toast is dismissed (xmark / swipe / auto): `(ctx, id)`.
type OnDismiss = unsafe extern "C" fn(ctx: *mut c_void, id: u64);

/// A live toast: its id, kind, and the retained card + content stack used
/// for layout/measurement.
struct ToastEntry {
    id: u64,
    card: Retained<UIView>,
    content: Retained<UIStackView>,
}

#[derive(Default)]
pub struct Ivars {
    entries: RefCell<Vec<ToastEntry>>,
    next_id: Cell<u64>,
    on_action: Cell<Option<OnAction>>,
    on_dismiss: Cell<Option<OnDismiss>>,
    /// Opaque host context. SAFETY: never dereffed on the Rust side.
    ctx: Cell<*mut c_void>,
}

// SAFETY: only ever touched on the main thread (MainThreadOnly).
unsafe impl Send for Ivars {}
unsafe impl Sync for Ivars {}

define_class!(
    #[unsafe(super(UIView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BtIosToasterView"]
    #[ivars = Ivars]
    pub struct BtIosToasterView;

    impl BtIosToasterView {
        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(this: Allocated<Self>, frame: CGRect) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send![super(this), initWithFrame: frame] }
        }

        /// Pass through touches that don't land on a card so the toaster
        /// container never blocks the nav stack underneath it (parity with
        /// the SwiftUI overlay's `allowsHitTesting` gating).
        #[unsafe(method(hitTest:withEvent:))]
        fn hit_test(&self, point: CGPoint, event: *mut AnyObject) -> *mut UIView {
            let result: *mut UIView =
                unsafe { msg_send![super(self), hitTest: point, withEvent: event] };
            let self_ptr = self as *const Self as *const UIView;
            if result.cast_const() == self_ptr {
                std::ptr::null_mut()
            } else {
                result
            }
        }

        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), layoutSubviews] };
            self.relayout_stack(false);
        }

        // ---- Selector handlers -------------------------------------------

        #[unsafe(method(dismissButtonTapped:))]
        fn dismiss_button_tapped(&self, sender: &AnyObject) {
            let tag: i64 = unsafe { msg_send![sender, tag] };
            self.dismiss(tag as u64);
        }

        #[unsafe(method(actionButtonTapped:))]
        fn action_button_tapped(&self, sender: &AnyObject) {
            let tag: i64 = unsafe { msg_send![sender, tag] };
            if tag < 0 {
                return;
            }
            let id = (tag / 1000) as u64;
            let index = (tag % 1000) as u32;
            if let Some(cb) = self.ivars().on_action.get() {
                let ctx = self.ivars().ctx.get();
                // Parity with the SwiftUI overlay: tapping an action runs
                // its handler but does NOT auto-dismiss the toast.
                unsafe { cb(ctx, id, index) };
            }
        }

        #[unsafe(method(cardPanned:))]
        fn card_panned(&self, sender: &AnyObject) {
            let state: i64 = unsafe { msg_send![sender, state] };
            if state != GESTURE_STATE_ENDED {
                return;
            }
            let view: *mut UIView = unsafe { msg_send![sender, view] };
            if view.is_null() {
                return;
            }
            let translation: CGPoint = unsafe { msg_send![sender, translationInView: view] };
            if translation.y >= SWIPE_DISMISS_THRESHOLD {
                return;
            }
            let id = {
                let entries = self.ivars().entries.borrow();
                entries
                    .iter()
                    .find(|e| std::ptr::eq(&*e.card as *const UIView, view.cast_const()))
                    .map(|e| e.id)
            };
            if let Some(id) = id {
                self.dismiss(id);
            }
        }
    }
);

impl BtIosToasterView {
    pub(crate) fn set_callbacks(
        &self,
        on_action: Option<OnAction>,
        on_dismiss: Option<OnDismiss>,
        ctx: *mut c_void,
    ) {
        self.ivars().on_action.set(on_action);
        self.ivars().on_dismiss.set(on_dismiss);
        self.ivars().ctx.set(ctx);
    }

    /// Lay out the toast stack to match sonner's collapsed look:
    ///
    /// - The front card (depth 0) sits at its natural measured height.
    /// - Back cards are clipped to the *front* card's height (so their own
    ///   content never peeks), scaled progressively narrower, faded, and
    ///   nudged a fixed gap each so only a thin sliver shows below the
    ///   front card — all nesting from the front card's top edge.
    ///
    /// `UIView`'s default transform scales about the view centre, so we
    /// frame every visible card at the front card's rect (top = `TOP_PAD`)
    /// and use the transform purely for scale + the peek translate,
    /// compensating the centre-scale top shift so top edges nest.
    fn relayout_stack(&self, animated: bool) {
        let bounds: CGRect = unsafe { msg_send![self, bounds] };
        let width = (bounds.size.width - 2.0 * H_MARGIN).max(0.0);

        // Snapshot owned handles + depth so the (possibly `'static`)
        // animation closure doesn't borrow the `entries` `RefCell`.
        let plan: Vec<(Retained<UIView>, Retained<UIStackView>, usize)> = {
            let entries = self.ivars().entries.borrow();
            let n = entries.len();
            if n == 0 {
                return;
            }
            entries
                .iter()
                .enumerate()
                // Newest (last pushed) is frontmost: depth 0.
                .map(|(i, e)| (e.card.clone(), e.content.clone(), n - 1 - i))
                .collect()
        };

        // Measure the front (depth 0) card's natural height; every visible
        // card is framed to this height so back cards clip to it.
        let front_h = plan
            .iter()
            .find(|(_, _, depth)| *depth == 0)
            .map(|(_, content, _)| Self::measure_card_height(content, width))
            .unwrap_or(0.0);

        let apply = move || {
            for (card, content, depth) in &plan {
                let depth = *depth;
                if depth >= MAX_VISIBLE {
                    let _: () = unsafe { msg_send![&**card, setHidden: true] };
                    continue;
                }
                let _: () = unsafe { msg_send![&**card, setHidden: false] };

                // Back cards clip to the front height so their content can't
                // peek; the front card shows its full content.
                let clip = depth > 0;
                let _: () = unsafe { msg_send![&**card, setClipsToBounds: clip] };

                // Reset transform before reframing so the frame is set in
                // untransformed coordinates, then re-apply scale + translate.
                let _: () =
                    unsafe { msg_send![&**card, setTransform: CGAffineTransform::IDENTITY] };

                let card_frame = CGRect {
                    origin: CGPoint {
                        x: H_MARGIN,
                        y: TOP_PAD,
                    },
                    size: CGSize {
                        width,
                        height: front_h,
                    },
                };
                let _: () = unsafe { msg_send![&**card, setFrame: card_frame] };

                let content_frame = CGRect {
                    origin: CGPoint {
                        x: CARD_PAD,
                        y: CARD_PAD,
                    },
                    size: CGSize {
                        width: (width - 2.0 * CARD_PAD).max(0.0),
                        height: (front_h - 2.0 * CARD_PAD).max(0.0),
                    },
                };
                let _: () = unsafe { msg_send![&**content, setFrame: content_frame] };
                // Only the front card shows its content; back cards reveal
                // just their card chrome (rounded edge) in the peek sliver,
                // matching sonner's collapsed stack.
                let content_alpha: CGFloat = if depth == 0 { 1.0 } else { 0.0 };
                let _: () = unsafe { msg_send![&**content, setAlpha: content_alpha] };

                // Scale about centre; compensate the resulting top-edge
                // shift (`front_h*(1-s)/2`) so top edges nest, then nudge
                // down by the fixed peek gap.
                let s = scale_for(depth);
                let ty = offset_for(depth) - front_h * (1.0 - s) / 2.0;
                let transform = CGAffineTransform::scale_translate_y(s, ty);
                let _: () = unsafe { msg_send![&**card, setTransform: transform] };
                let _: () = unsafe { msg_send![&**card, setAlpha: opacity_for(depth) as CGFloat] };
            }
        };

        if animated {
            Self::animate(apply);
        } else {
            apply();
        }
    }

    /// Measure a card's content stack at `width` to derive the card height
    /// (content + vertical padding). Horizontal priority required, vertical
    /// at fitting-size level so the wrapped description height is exact.
    fn measure_card_height(content: &UIStackView, width: CGFloat) -> CGFloat {
        let fitting = CGSize { width, height: 0.0 };
        let natural: CGSize = unsafe {
            msg_send![
                content,
                systemLayoutSizeFittingSize: fitting,
                withHorizontalFittingPriority: 1000.0_f32,
                verticalFittingPriority: 50.0_f32,
            ]
        };
        natural.height + 2.0 * CARD_PAD
    }

    /// Run `body` inside a smooth ease-out `UIView` animation.
    fn animate(body: impl Fn() + Clone + 'static) {
        use block2::{RcBlock, StackBlock};
        let block = StackBlock::new(body);
        let block: RcBlock<dyn Fn()> = block.copy();
        let cls = objc2::class!(UIView);
        let _: () = unsafe {
            msg_send![
                cls,
                animateWithDuration: ANIM_DURATION,
                delay: 0.0_f64,
                options: ANIM_OPTIONS,
                animations: &*block,
                completion: std::ptr::null_mut::<AnyObject>(),
            ]
        };
    }

    /// Animate `card` out (fade to 0 + shrink to [`DISMISS_SCALE`]) and run
    /// `done(card)` on completion (used to remove it from the superview).
    fn animate_out(card: Retained<UIView>, done: impl Fn(Retained<UIView>) + Clone + 'static) {
        use block2::{RcBlock, StackBlock};
        let anim_card = card.clone();
        let anim = StackBlock::new(move || {
            let _: () = unsafe { msg_send![&*anim_card, setAlpha: 0.0_f64 as CGFloat] };
            let _: () = unsafe {
                msg_send![&*anim_card, setTransform: CGAffineTransform::scale_translate_y(DISMISS_SCALE, -INSERT_SLIDE)]
            };
        });
        let anim: RcBlock<dyn Fn()> = anim.copy();
        let completion = StackBlock::new(move |_finished: objc2::runtime::Bool| {
            done(card.clone());
        });
        let completion: RcBlock<dyn Fn(objc2::runtime::Bool)> = completion.copy();
        let cls = objc2::class!(UIView);
        let _: () = unsafe {
            msg_send![
                cls,
                animateWithDuration: ANIM_DURATION,
                delay: 0.0_f64,
                options: ANIM_OPTIONS,
                animations: &*anim,
                completion: &*completion,
            ]
        };
    }

    /// Append a toast and return its id. Builds the card, mounts it
    /// frontmost, and schedules auto-dismiss when applicable.
    pub(crate) fn show(
        &self,
        kind: ToastKind,
        title: &str,
        description: Option<&str>,
        persistent: bool,
        actions: &[ToastAction],
    ) -> u64 {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let id = self.ivars().next_id.get().wrapping_add(1);
        self.ivars().next_id.set(id);

        let ToastCardViews { card, content } = make_toast_card(
            mtm,
            id,
            kind,
            title,
            description,
            persistent,
            actions,
            self.as_ref(),
            sel!(dismissButtonTapped:),
            sel!(actionButtonTapped:),
            sel!(cardPanned:),
        );

        // addSubview puts the card frontmost → newest renders on top.
        let _: () = unsafe { msg_send![self, addSubview: &*card] };

        // Pre-position the incoming front card above its resting spot and
        // transparent, so the relayout animates it sliding down + fading in
        // while the existing cards settle into their new stacked transforms.
        let _: () = unsafe { msg_send![&*card, setAlpha: 0.0_f64 as CGFloat] };
        let _: () = unsafe {
            msg_send![&*card, setTransform: CGAffineTransform::scale_translate_y(1.0, -INSERT_SLIDE)]
        };

        self.ivars()
            .entries
            .borrow_mut()
            .push(ToastEntry { id, card, content });

        // Animate the whole stack (incoming + repositioned existing cards).
        self.relayout_stack(true);

        if should_autodismiss(kind, persistent) {
            self.schedule_autodismiss(id);
        }
        id
    }

    pub(crate) fn dismiss(&self, id: u64) {
        let removed = {
            let mut entries = self.ivars().entries.borrow_mut();
            let pos = entries.iter().position(|e| e.id == id);
            pos.map(|pos| entries.remove(pos))
        };
        let Some(entry) = removed else { return };

        // Animate the leaving card out (fade + slight shrink) and remove it
        // on completion; concurrently settle the remaining stack.
        let leaving = entry.card.clone();
        Self::animate_out(leaving, move |card| {
            let _: () = unsafe { msg_send![&*card, removeFromSuperview] };
        });
        self.relayout_stack(true);

        if let Some(cb) = self.ivars().on_dismiss.get() {
            unsafe { cb(self.ivars().ctx.get(), id) };
        }
    }

    pub(crate) fn dismiss_all(&self) {
        let drained: Vec<ToastEntry> = self.ivars().entries.borrow_mut().drain(..).collect();
        for entry in &drained {
            let _: () = unsafe { msg_send![&*entry.card, removeFromSuperview] };
            if let Some(cb) = self.ivars().on_dismiss.get() {
                unsafe { cb(self.ivars().ctx.get(), entry.id) };
            }
        }
        let _: () = unsafe { msg_send![self, setNeedsLayout] };
    }

    /// Schedule auto-dismissal of `id` on the main queue after
    /// [`AUTO_DISMISS_SECS`]. The fire is a no-op if the toast was already
    /// dismissed (ids are monotonic, so no reuse confusion). Mirrors the
    /// `dispatch_after_f` idiom in `coordinator::chips`.
    fn schedule_autodismiss(&self, id: u64) {
        let self_ptr: *const Self = self;
        let payload = Box::into_raw(Box::new((self_ptr, id)));
        extern "C" {
            fn dispatch_after_f(
                when: u64,
                queue: *mut c_void,
                ctx: *mut c_void,
                work: unsafe extern "C" fn(*mut c_void),
            );
            fn dispatch_time(when: u64, delta: i64) -> u64;
            static _dispatch_main_q: c_void;
        }
        const DISPATCH_TIME_NOW: u64 = 0;
        const NSEC_PER_SEC: f64 = 1_000_000_000.0;

        unsafe extern "C" fn fire(ctx: *mut c_void) {
            // SAFETY: box leaked in `schedule_autodismiss`; recover here.
            let boxed = unsafe { Box::from_raw(ctx as *mut (*const BtIosToasterView, u64)) };
            let (self_ptr, id) = *boxed;
            // SAFETY: the toaster view is retained by the window for the
            // app lifetime and the main queue runs on its thread.
            let view = unsafe { &*self_ptr };
            view.dismiss(id);
        }

        let delta = (super::AUTO_DISMISS_SECS * NSEC_PER_SEC) as i64;
        unsafe {
            let when = dispatch_time(DISPATCH_TIME_NOW, delta);
            dispatch_after_f(
                when,
                &_dispatch_main_q as *const _ as *mut c_void,
                payload as *mut c_void,
                fire,
            );
        }
    }
}

/// Construct a `BtIosToasterView` with a zero frame (the caller adds it to
/// the window and constrains it). Returns a +1 retained instance.
pub(crate) fn create_toaster_view() -> Retained<BtIosToasterView> {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    unsafe { msg_send![mtm.alloc::<BtIosToasterView>(), initWithFrame: CGRect::default()] }
}
