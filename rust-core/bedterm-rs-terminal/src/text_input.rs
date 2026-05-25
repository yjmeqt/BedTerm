//! UITextInput protocol conformance for the Metal-backed view1.
//!
//! Phase 1 lives in three pieces:
//!   * `BtRsUITextPosition` — `UITextPosition` subclass storing a byte index.
//!   * `BtRsUITextRange`    — `UITextRange` subclass holding start/end positions.
//!   * `UITextInput` method bodies on `BtRsMetalInputView` (see `metal_view.rs`).

use objc2::rc::{Allocated, Retained};
use objc2::{declare_class, msg_send_id, ClassType, DeclaredClass};
use objc2_foundation::MainThreadMarker;
use objc2_ui_kit::{UITextPosition, UITextRange};
use std::cell::{Cell, RefCell};

// -------- BtRsUITextPosition ------------------------------------------------

#[derive(Default)]
pub struct PositionIvars {
    /// Byte index into the owning view's text buffer.
    pub index: Cell<usize>,
}

unsafe impl Send for PositionIvars {}
unsafe impl Sync for PositionIvars {}

declare_class!(
    pub struct BtRsUITextPosition;

    unsafe impl ClassType for BtRsUITextPosition {
        type Super = UITextPosition;
        type Mutability = objc2::mutability::MainThreadOnly;
        const NAME: &'static str = "BtRsUITextPosition";
    }

    impl DeclaredClass for BtRsUITextPosition {
        type Ivars = PositionIvars;
    }

    unsafe impl BtRsUITextPosition {
        #[method_id(init)]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(PositionIvars::default());
            unsafe { msg_send_id![super(this), init] }
        }
    }
);

impl BtRsUITextPosition {
    pub fn new(mtm: MainThreadMarker, index: usize) -> Retained<Self> {
        let this: Retained<Self> = unsafe { msg_send_id![mtm.alloc::<Self>(), init] };
        this.ivars().index.set(index);
        this
    }

    pub fn index(&self) -> usize {
        self.ivars().index.get()
    }

    /// Upcast to the parent class without bumping the retain count beyond the
    /// caller's expectation. `Retained::cast` is safe because the runtime
    /// guarantees Self IS-A UITextPosition.
    pub fn into_super(this: Retained<Self>) -> Retained<UITextPosition> {
        // SAFETY: BtRsUITextPosition is a subclass of UITextPosition.
        unsafe { Retained::cast(this) }
    }
}

// -------- BtRsUITextRange ---------------------------------------------------

#[derive(Default)]
pub struct RangeIvars {
    pub start: RefCell<Option<Retained<BtRsUITextPosition>>>,
    pub end: RefCell<Option<Retained<BtRsUITextPosition>>>,
}

unsafe impl Send for RangeIvars {}
unsafe impl Sync for RangeIvars {}

declare_class!(
    pub struct BtRsUITextRange;

    unsafe impl ClassType for BtRsUITextRange {
        type Super = UITextRange;
        type Mutability = objc2::mutability::MainThreadOnly;
        const NAME: &'static str = "BtRsUITextRange";
    }

    impl DeclaredClass for BtRsUITextRange {
        type Ivars = RangeIvars;
    }

    unsafe impl BtRsUITextRange {
        #[method_id(init)]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(RangeIvars::default());
            unsafe { msg_send_id![super(this), init] }
        }

        #[method_id(start)]
        fn start_override(&self) -> Option<Retained<UITextPosition>> {
            self.ivars()
                .start
                .borrow()
                .as_ref()
                .map(|p| BtRsUITextPosition::into_super(p.clone()))
        }

        #[method_id(end)]
        fn end_override(&self) -> Option<Retained<UITextPosition>> {
            self.ivars()
                .end
                .borrow()
                .as_ref()
                .map(|p| BtRsUITextPosition::into_super(p.clone()))
        }

        #[method(isEmpty)]
        fn is_empty_override(&self) -> bool {
            let s = self.start_index();
            let e = self.end_index();
            s == e
        }
    }
);

impl BtRsUITextRange {
    pub fn new(
        mtm: MainThreadMarker,
        start: Retained<BtRsUITextPosition>,
        end: Retained<BtRsUITextPosition>,
    ) -> Retained<Self> {
        let this: Retained<Self> = unsafe { msg_send_id![mtm.alloc::<Self>(), init] };
        *this.ivars().start.borrow_mut() = Some(start);
        *this.ivars().end.borrow_mut() = Some(end);
        this
    }

    /// Byte index of the start position, or 0 if unset.
    pub fn start_index(&self) -> usize {
        self.ivars()
            .start
            .borrow()
            .as_ref()
            .map(|p| p.index())
            .unwrap_or(0)
    }

    /// Byte index of the end position, or 0 if unset.
    pub fn end_index(&self) -> usize {
        self.ivars()
            .end
            .borrow()
            .as_ref()
            .map(|p| p.index())
            .unwrap_or(0)
    }
}
