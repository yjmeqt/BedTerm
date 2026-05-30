//! Tappable list-row component used by the W24b Rust Hosts list VC.
//!
//! Two-line title + subtitle, optional trailing chevron, full-row tap
//! target, optional left-swipe → delete action. Not a `UITableViewCell`
//! — the Hosts VC drives layout with a plain `UIScrollView` + vertical
//! stack, so this is just a card-like `UIView` factory plus a tiny
//! [`ListRowHandle`] for installing per-row callbacks after construction.
//!
//! Layout constants live in [`bedterm_app::design_system::list_row_metrics::ListRowMetrics`]
//! (pure-Rust, host-testable).

use crate::design_system::{colors, typography};
use bedterm_app::design_system::list_row_metrics::ListRowMetrics;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, ClassType};
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::{
    NSDirectionalEdgeInsets, UIButton, UIImage, UIImageSymbolConfiguration, UIImageSymbolWeight,
    UIImageView, UILabel, UILayoutConstraintAxis, UIStackView, UIStackViewAlignment,
    UIStackViewDistribution, UISwipeGestureRecognizer, UISwipeGestureRecognizerDirection, UIView,
};

/// Trailing accessory drawn on the row. Matches the SwiftUI `HostRow`
/// auth badge (key / lock icon inside a bordered square) so the Rust
/// hosts VC stays visually identical when `useRustHostsList` is on.
#[derive(Clone, Copy)]
pub enum ListRowAccessory {
    /// A right-pointing chevron (e.g. Settings rows).
    Chevron,
    /// SF Symbol badge wrapped in a bordered square. `system_name` is
    /// the SF Symbol identifier (e.g. `"key.fill"` / `"lock.fill"`).
    Badge { system_name: &'static str },
    /// No trailing accessory.
    None,
}

/// `UIControlEventTouchUpInside` = 1 << 6 (same constant the
/// `choice_button` factory uses).
const CONTROL_EVENT_TOUCH_UP_INSIDE: u64 = 1 << 6;
/// `NSTextAlignment.left` = 0.
const TEXT_ALIGNMENT_LEFT: i64 = 0;

/// Opaque handle returned alongside the row view so callers can wire
/// taps and swipe-delete after construction. Holds retained pointers to
/// the tap-target `UIButton` and the outer row view (where the swipe
/// gesture is installed).
pub struct ListRowHandle {
    pub(crate) button: Retained<UIButton>,
    pub(crate) row_view: Retained<UIView>,
}

impl ListRowHandle {
    /// Install a tap handler. `target` / `action` are stored on the
    /// underlying `UIButton` (which sits behind the labels with all
    /// subviews user-interaction-disabled, so a tap anywhere on the row
    /// routes through).
    pub fn set_on_tap(&self, target: &AnyObject, action: Sel) {
        let _: () = unsafe {
            msg_send![
                &*self.button,
                addTarget: target,
                action: action,
                forControlEvents: CONTROL_EVENT_TOUCH_UP_INSIDE,
            ]
        };
    }

    /// Install a left-swipe delete handler. Selector signature should be
    /// `@objc func _(_ sender: UISwipeGestureRecognizer)`.
    pub fn set_on_delete(&self, mtm: MainThreadMarker, target: &AnyObject, action: Sel) {
        let recognizer: Retained<UISwipeGestureRecognizer> = unsafe {
            let alloc = mtm.alloc::<UISwipeGestureRecognizer>();
            msg_send![alloc, initWithTarget: target, action: action]
        };
        recognizer.setDirection(UISwipeGestureRecognizerDirection::Left);
        let _: () = unsafe { msg_send![&*self.row_view, addGestureRecognizer: &*recognizer] };
    }
}

/// Build a tappable two-line list row.
pub fn make_list_row(
    mtm: MainThreadMarker,
    title: &str,
    subtitle: Option<&str>,
    show_chevron: bool,
) -> (Retained<UIView>, ListRowHandle) {
    let accessory = if show_chevron {
        ListRowAccessory::Chevron
    } else {
        ListRowAccessory::None
    };
    make_list_row_with_accessory(mtm, title, subtitle, accessory)
}

/// Variant of [`make_list_row`] taking a [`ListRowAccessory`]. Used by
/// the Hosts list to draw a key/lock badge instead of a chevron.
pub fn make_list_row_with_accessory(
    mtm: MainThreadMarker,
    title: &str,
    subtitle: Option<&str>,
    accessory: ListRowAccessory,
) -> (Retained<UIView>, ListRowHandle) {
    // Outer card — owns the border, corner radius, background.
    let card = UIView::new(mtm);
    card.setBackgroundColor(Some(&colors::shadcn_card()));
    let layer: Retained<AnyObject> = unsafe { msg_send![&*card, layer] };
    unsafe {
        let _: () = msg_send![&*layer, setCornerRadius: ListRowMetrics::CORNER_RADIUS];
        let _: () = msg_send![&*layer, setBorderWidth: ListRowMetrics::BORDER_WIDTH];
        let _: () = msg_send![&*layer, setMasksToBounds: true];
        let border = colors::shadcn_border();
        let cg: *const AnyObject = msg_send![&*border, CGColor];
        let _: () = msg_send![&*layer, setBorderColor: cg];
    }

    // Tap-target UIButton fills the card. Subviews live above it with
    // userInteractionEnabled=false so the button receives every tap
    // regardless of where on the row the user pressed.
    let button: Retained<UIButton> = unsafe { msg_send![UIButton::class(), buttonWithType: 0_i64] };
    let button_view = unsafe { &*(&*button as *const UIButton as *const UIView) };
    button_view.setBackgroundColor(None);

    // Title label. SwiftUI HostRow uses `.callout.weight(.medium)` →
    // 16pt medium; mirror that here (not 17pt body) to keep parity when
    // the Rust list flag is on.
    let title_label = UILabel::new(mtm);
    title_label.setText(Some(&NSString::from_str(title)));
    unsafe {
        title_label.setFont(Some(&typography::callout_medium()));
        title_label.setTextColor(Some(&colors::shadcn_primary()));
    }
    title_label.setNumberOfLines(1);
    let _: () = unsafe { msg_send![&*title_label, setTextAlignment: TEXT_ALIGNMENT_LEFT] };

    // Subtitle label (optional). SwiftUI HostRow uses `.footnote` (13pt).
    let subtitle_label_opt: Option<Retained<UILabel>> = subtitle.map(|s| {
        let label = UILabel::new(mtm);
        label.setText(Some(&NSString::from_str(s)));
        unsafe {
            label.setFont(Some(&typography::footnote()));
            label.setTextColor(Some(&colors::shadcn_muted_foreground()));
        }
        label.setNumberOfLines(1);
        let _: () = unsafe { msg_send![&*label, setTextAlignment: TEXT_ALIGNMENT_LEFT] };
        label
    });

    // Vertical text stack.
    let text_stack = UIStackView::new(mtm);
    text_stack.setAxis(UILayoutConstraintAxis::Vertical);
    text_stack.setAlignment(UIStackViewAlignment::Leading);
    text_stack.setDistribution(UIStackViewDistribution::Fill);
    text_stack.setSpacing(ListRowMetrics::TITLE_SUBTITLE_SPACING);
    text_stack.addArrangedSubview(unsafe { &*(&*title_label as *const UILabel as *const UIView) });
    if let Some(sub) = subtitle_label_opt.as_ref() {
        text_stack.addArrangedSubview(unsafe { &*(&**sub as *const UILabel as *const UIView) });
    }

    // Optional trailing accessory — chevron, key/lock badge, or none.
    // The chevron path is unchanged from prior behaviour; the badge
    // path wraps the SF symbol in a bordered square to mirror the
    // SwiftUI `HostRow.authBadge` look (caption2-semibold glyph inside
    // a 24x24 square stroked with the `ShadcnBorder` token).
    enum TrailingView {
        None,
        Chevron(Retained<UIImageView>),
        Badge(Retained<UIView>),
    }
    let trailing: TrailingView = match accessory {
        ListRowAccessory::None => TrailingView::None,
        ListRowAccessory::Chevron => {
            let chevron_ns = NSString::from_str("chevron.right");
            let chevron_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
                ListRowMetrics::CHEVRON_POINT_SIZE,
                UIImageSymbolWeight::Semibold,
            );
            let view = UIImageView::new(mtm);
            if let Some(image) = UIImage::systemImageNamed(&chevron_ns) {
                let configured: Retained<UIImage> = unsafe {
                    msg_send![&*image, imageByApplyingSymbolConfiguration: &*chevron_cfg]
                };
                view.setImage(Some(&configured));
            }
            unsafe {
                let tint = colors::shadcn_muted_foreground();
                let _: () = msg_send![&*view, setTintColor: &*tint];
            }
            TrailingView::Chevron(view)
        }
        ListRowAccessory::Badge { system_name } => {
            // Outer bordered square (24x24).
            let container = UIView::new(mtm);
            container.setBackgroundColor(None);
            let layer: Retained<AnyObject> = unsafe { msg_send![&*container, layer] };
            unsafe {
                let _: () =
                    msg_send![&*layer, setCornerRadius: ListRowMetrics::BADGE_CORNER_RADIUS];
                let _: () = msg_send![&*layer, setBorderWidth: ListRowMetrics::BORDER_WIDTH];
                let border = colors::shadcn_border();
                let cg: *const AnyObject = msg_send![&*border, CGColor];
                let _: () = msg_send![&*layer, setBorderColor: cg];
            }
            // Glyph inside the square.
            let glyph_ns = NSString::from_str(system_name);
            let glyph_cfg = UIImageSymbolConfiguration::configurationWithPointSize_weight(
                ListRowMetrics::BADGE_GLYPH_POINT_SIZE,
                UIImageSymbolWeight::Semibold,
            );
            let image_view = UIImageView::new(mtm);
            if let Some(image) = UIImage::systemImageNamed(&glyph_ns) {
                let configured: Retained<UIImage> =
                    unsafe { msg_send![&*image, imageByApplyingSymbolConfiguration: &*glyph_cfg] };
                image_view.setImage(Some(&configured));
            }
            unsafe {
                let tint = colors::shadcn_muted_foreground();
                let _: () = msg_send![&*image_view, setTintColor: &*tint];
                let _: () =
                    msg_send![&*image_view, setTranslatesAutoresizingMaskIntoConstraints: false];
                let _: () =
                    msg_send![&*container, setTranslatesAutoresizingMaskIntoConstraints: false];
            }
            // Pin 24x24 + center glyph.
            unsafe {
                let w_anchor: Retained<AnyObject> = msg_send![&*container, widthAnchor];
                let h_anchor: Retained<AnyObject> = msg_send![&*container, heightAnchor];
                let w: Retained<AnyObject> =
                    msg_send![&*w_anchor, constraintEqualToConstant: ListRowMetrics::BADGE_SIZE];
                let h: Retained<AnyObject> =
                    msg_send![&*h_anchor, constraintEqualToConstant: ListRowMetrics::BADGE_SIZE];
                let _: () = msg_send![&*w, setActive: true];
                let _: () = msg_send![&*h, setActive: true];
            }
            let img_view: &UIView =
                unsafe { &*(&*image_view as *const UIImageView as *const UIView) };
            let _: () = unsafe { msg_send![&*container, addSubview: img_view] };
            unsafe {
                let c_cx: Retained<AnyObject> = msg_send![&*container, centerXAnchor];
                let c_cy: Retained<AnyObject> = msg_send![&*container, centerYAnchor];
                let i_cx: Retained<AnyObject> = msg_send![&*image_view, centerXAnchor];
                let i_cy: Retained<AnyObject> = msg_send![&*image_view, centerYAnchor];
                let k1: Retained<AnyObject> = msg_send![&*i_cx, constraintEqualToAnchor: &*c_cx];
                let k2: Retained<AnyObject> = msg_send![&*i_cy, constraintEqualToAnchor: &*c_cy];
                let _: () = msg_send![&*k1, setActive: true];
                let _: () = msg_send![&*k2, setActive: true];
            }
            TrailingView::Badge(container)
        }
    };

    // Outer horizontal stack with directional margins.
    let outer = UIStackView::new(mtm);
    outer.setAxis(UILayoutConstraintAxis::Horizontal);
    outer.setAlignment(UIStackViewAlignment::Center);
    outer.setDistribution(UIStackViewDistribution::Fill);
    outer.setSpacing(ListRowMetrics::TEXT_TO_ACCESSORY_SPACING);
    outer.setLayoutMarginsRelativeArrangement(true);
    outer.setDirectionalLayoutMargins(NSDirectionalEdgeInsets {
        top: ListRowMetrics::VERTICAL_PADDING,
        leading: ListRowMetrics::HORIZONTAL_PADDING,
        bottom: ListRowMetrics::VERTICAL_PADDING,
        trailing: ListRowMetrics::HORIZONTAL_PADDING,
    });
    outer.addArrangedSubview(unsafe { &*(&*text_stack as *const UIStackView as *const UIView) });
    let trailing_uiview: Option<&UIView> = match &trailing {
        TrailingView::None => None,
        TrailingView::Chevron(v) => {
            Some(unsafe { &*(&**v as *const UIImageView as *const UIView) })
        }
        TrailingView::Badge(v) => Some(&**v),
    };
    if let Some(v) = trailing_uiview {
        outer.addArrangedSubview(v);
    }

    // Hugging priorities so the trailing accessory sticks to the
    // trailing edge.
    unsafe {
        let _: () = msg_send![&*text_stack, setContentHuggingPriority: 249_f32, forAxis: 0_i64];
        if let Some(v) = trailing_uiview {
            let _: () = msg_send![v, setContentHuggingPriority: 1000_f32, forAxis: 0_i64];
            let _: () =
                msg_send![v, setContentCompressionResistancePriority: 1000_f32, forAxis: 0_i64];
        }
    }

    // Disable subview interaction so the button beneath gets every tap.
    outer.setUserInteractionEnabled(false);
    let _: () = unsafe { msg_send![&*outer, setTranslatesAutoresizingMaskIntoConstraints: false] };
    let _: () = unsafe { msg_send![&*button, setTranslatesAutoresizingMaskIntoConstraints: false] };

    // Mount: card > button (fills card) > outer (fills button).
    card.addSubview(button_view);
    let _: () = unsafe { msg_send![&*button, addSubview: &*outer] };

    // Auto Layout: pin button to card, outer to button.
    unsafe fn pin_to_superview(child: &AnyObject, parent: &AnyObject) {
        let c_top: Retained<AnyObject> = msg_send![child, topAnchor];
        let c_bot: Retained<AnyObject> = msg_send![child, bottomAnchor];
        let c_lead: Retained<AnyObject> = msg_send![child, leadingAnchor];
        let c_trail: Retained<AnyObject> = msg_send![child, trailingAnchor];
        let p_top: Retained<AnyObject> = msg_send![parent, topAnchor];
        let p_bot: Retained<AnyObject> = msg_send![parent, bottomAnchor];
        let p_lead: Retained<AnyObject> = msg_send![parent, leadingAnchor];
        let p_trail: Retained<AnyObject> = msg_send![parent, trailingAnchor];
        let k1: Retained<AnyObject> = msg_send![&*c_top, constraintEqualToAnchor: &*p_top];
        let k2: Retained<AnyObject> = msg_send![&*c_bot, constraintEqualToAnchor: &*p_bot];
        let k3: Retained<AnyObject> = msg_send![&*c_lead, constraintEqualToAnchor: &*p_lead];
        let k4: Retained<AnyObject> = msg_send![&*c_trail, constraintEqualToAnchor: &*p_trail];
        let _: () = msg_send![&*k1, setActive: true];
        let _: () = msg_send![&*k2, setActive: true];
        let _: () = msg_send![&*k3, setActive: true];
        let _: () = msg_send![&*k4, setActive: true];
    }
    unsafe {
        pin_to_superview(button_view as &AnyObject, &*card as &AnyObject);
        pin_to_superview(
            &*outer as &UIStackView as &AnyObject,
            &*button as &UIButton as &AnyObject,
        );
    }

    let row_view: Retained<UIView> = unsafe { Retained::cast_unchecked::<UIView>(card) };
    let handle = ListRowHandle {
        button: button.clone(),
        row_view: row_view.clone(),
    };
    (row_view, handle)
}
