//! UITextInput protocol conformance for the Metal-backed view1.
//!
//! Phase 1 lives in three pieces:
//!   * `BtIosUITextPosition` — `UITextPosition` subclass storing a byte index.
//!   * `BtIosUITextRange`    — `UITextRange` subclass holding start/end positions.
//!   * `UITextInput` method bodies on `BtIosMetalInputView` (see `metal_view.rs`).

use objc2::rc::{Allocated, Retained};
use objc2::{declare_class, msg_send_id, ClassType, DeclaredClass};
use objc2_foundation::MainThreadMarker;
use objc2_ui_kit::{UITextPosition, UITextRange};
use std::cell::{Cell, RefCell};

// -------- BtIosUITextPosition ------------------------------------------------

#[derive(Default)]
pub struct PositionIvars {
    /// Byte index into the owning view's text buffer.
    pub index: Cell<usize>,
}

unsafe impl Send for PositionIvars {}
unsafe impl Sync for PositionIvars {}

declare_class!(
    pub struct BtIosUITextPosition;

    unsafe impl ClassType for BtIosUITextPosition {
        type Super = UITextPosition;
        type Mutability = objc2::mutability::MainThreadOnly;
        const NAME: &'static str = "BtIosUITextPosition";
    }

    impl DeclaredClass for BtIosUITextPosition {
        type Ivars = PositionIvars;
    }

    unsafe impl BtIosUITextPosition {
        #[method_id(init)]
        fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
            let this = this.set_ivars(PositionIvars::default());
            unsafe { msg_send_id![super(this), init] }
        }
    }
);

impl BtIosUITextPosition {
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
        // SAFETY: BtIosUITextPosition is a subclass of UITextPosition.
        unsafe { Retained::cast(this) }
    }
}

// -------- BtIosUITextRange ---------------------------------------------------

#[derive(Default)]
pub struct RangeIvars {
    pub start: RefCell<Option<Retained<BtIosUITextPosition>>>,
    pub end: RefCell<Option<Retained<BtIosUITextPosition>>>,
}

unsafe impl Send for RangeIvars {}
unsafe impl Sync for RangeIvars {}

declare_class!(
    pub struct BtIosUITextRange;

    unsafe impl ClassType for BtIosUITextRange {
        type Super = UITextRange;
        type Mutability = objc2::mutability::MainThreadOnly;
        const NAME: &'static str = "BtIosUITextRange";
    }

    impl DeclaredClass for BtIosUITextRange {
        type Ivars = RangeIvars;
    }

    unsafe impl BtIosUITextRange {
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
                .map(|p| BtIosUITextPosition::into_super(p.clone()))
        }

        #[method_id(end)]
        fn end_override(&self) -> Option<Retained<UITextPosition>> {
            self.ivars()
                .end
                .borrow()
                .as_ref()
                .map(|p| BtIosUITextPosition::into_super(p.clone()))
        }

        #[method(isEmpty)]
        fn is_empty_override(&self) -> bool {
            let s = self.start_index();
            let e = self.end_index();
            s == e
        }
    }
);

impl BtIosUITextRange {
    pub fn new(
        mtm: MainThreadMarker,
        start: Retained<BtIosUITextPosition>,
        end: Retained<BtIosUITextPosition>,
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
