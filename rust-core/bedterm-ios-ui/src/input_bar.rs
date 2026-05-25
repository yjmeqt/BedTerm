//! State1 inline input bar — a single-line `UITextField` plus a unified
//! send chip on the right. The text field's delegate is the keyboard
//! coordinator (see `coordinator.rs`).

use crate::action_chip::make_action_chip;
use crate::coordinator::BtIosKeyboardCoordinator;
use crate::geometry::{CGFloat, CGPoint, CGRect, CGSize};
use objc2::rc::Retained;
use objc2::{msg_send, sel, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{UIColor, UITextField, UIView};

pub(crate) const INPUT_BAR_HEIGHT: CGFloat = 44.0;

const SEND_W: CGFloat = 72.0;
const H_PAD: CGFloat = 8.0;

/// Build the State1 input bar. Returns a container UIView with two
/// subviews — the UITextField (tag=1) and the send chip (tag=2) — laid
/// out manually in `viewDidLayoutSubviews` of the VC.
pub(crate) fn make_input_bar(
    mtm: MainThreadMarker,
    coordinator: &BtIosKeyboardCoordinator,
) -> Retained<UIView> {
    let frame = CGRect {
        origin: CGPoint::default(),
        size: CGSize {
            width: 320.0,
            height: INPUT_BAR_HEIGHT,
        },
    };
    let wrapper: Retained<UIView> =
        unsafe { msg_send![mtm.alloc::<UIView>(), initWithFrame: frame] };
    let bg: Retained<UIColor> = unsafe { msg_send![UIColor::class(), systemBackgroundColor] };
    let _: () = unsafe { msg_send![&*wrapper, setBackgroundColor: &*bg] };

    let field: Retained<UITextField> =
        unsafe { msg_send![mtm.alloc::<UITextField>(), initWithFrame: CGRect::default()] };
    let placeholder = NSString::from_str("Type a command");
    unsafe {
        field.setPlaceholder(Some(&placeholder));
        // borderStyle .roundedRect = 3.
        let _: () = msg_send![&*field, setBorderStyle: 3_i64];
        // The next setters are synthesised from the UITextInputTraits protocol.
        // objc2 0.6's `disable-encoding-assertions` feature (enabled in
        // Cargo.toml) lets `msg_send!` dispatch them directly — no raw IMP
        // fallback needed (see issue madsmtm/objc2#645).
        // returnKeyType .default = 0.
        let _: () = msg_send![&*field, setReturnKeyType: 0_i64];
        // autocorrectionType .no = 1.
        let _: () = msg_send![&*field, setAutocorrectionType: 1_i64];
        // autocapitalizationType .none = 0.
        let _: () = msg_send![&*field, setAutocapitalizationType: 0_i64];
        // smartDashesType .no = 1.
        let _: () = msg_send![&*field, setSmartDashesType: 1_i64];
        // smartQuotesType .no = 1.
        let _: () = msg_send![&*field, setSmartQuotesType: 1_i64];
        // smartInsertDeleteType .no = 1.
        let _: () = msg_send![&*field, setSmartInsertDeleteType: 1_i64];
        // delegate = coordinator (must conform to UITextFieldDelegate).
        let _: () = msg_send![&*field, setDelegate: coordinator];
        // tag for VC layout to find it.
        let _: () = msg_send![&*field, setTag: 1_i64];
    }
    let _: () = unsafe { msg_send![&*wrapper, addSubview: &*field] };

    // Send chip on the right.
    let target = coordinator.as_ref();
    let send = make_action_chip(
        mtm,
        "paperplane.fill",
        "Send",
        true,
        target,
        sel!(keybarSend:),
    );
    let _: () = unsafe { msg_send![&*send, setTag: 2_i64] };
    let _: () = unsafe { msg_send![&*wrapper, addSubview: &*send] };

    // Initial layout (autoresizing not enough — we set frames per-layout
    // pass in the VC, but seed something here so the bar isn't blank).
    let total_w = frame.size.width;
    let field_frame = CGRect {
        origin: CGPoint { x: H_PAD, y: 6.0 },
        size: CGSize {
            width: total_w - SEND_W - 3.0 * H_PAD,
            height: INPUT_BAR_HEIGHT - 12.0,
        },
    };
    let send_frame = CGRect {
        origin: CGPoint {
            x: total_w - SEND_W - H_PAD,
            y: 6.0,
        },
        size: CGSize {
            width: SEND_W,
            height: INPUT_BAR_HEIGHT - 12.0,
        },
    };
    let _: () = unsafe { msg_send![&*field, setFrame: field_frame] };
    let _: () = unsafe { msg_send![&*send, setFrame: send_frame] };

    // Coordinator needs a strong ref to the text field for read/clear.
    coordinator.set_input_field(&field);

    wrapper
}

/// Re-layout the input bar's children to fit a new outer width. The VC
/// calls this whenever the wrapper frame changes.
pub(crate) fn layout_input_bar_children(wrapper: &UIView) {
    let bounds: CGRect = unsafe { msg_send![wrapper, bounds] };
    let total_w = bounds.size.width;
    let total_h = bounds.size.height.max(INPUT_BAR_HEIGHT);

    // Locate subviews by tag.
    let field_ptr: *const objc2::runtime::AnyObject =
        unsafe { msg_send![wrapper, viewWithTag: 1_i64] };
    let send_ptr: *const objc2::runtime::AnyObject =
        unsafe { msg_send![wrapper, viewWithTag: 2_i64] };

    if !field_ptr.is_null() {
        let field_frame = CGRect {
            origin: CGPoint { x: H_PAD, y: 6.0 },
            size: CGSize {
                width: (total_w - SEND_W - 3.0 * H_PAD).max(0.0),
                height: total_h - 12.0,
            },
        };
        unsafe {
            let _: () = msg_send![field_ptr, setFrame: field_frame];
        }
    }
    if !send_ptr.is_null() {
        let send_frame = CGRect {
            origin: CGPoint {
                x: total_w - SEND_W - H_PAD,
                y: 6.0,
            },
            size: CGSize {
                width: SEND_W,
                height: total_h - 12.0,
            },
        };
        unsafe {
            let _: () = msg_send![send_ptr, setFrame: send_frame];
        }
    }
}
