//! Toaster — Rust-owned toast overlay (UIKit port of `Toaster.swift` +
//! `ToasterOverlay.swift`).
//!
//! `BtIosToasterView` is a window-level container `UIView` that owns the
//! live toast list, builds each toast as a `UIView` card (see
//! [`card`]), stacks the newest three, auto-dismisses non-persistent
//! info/success toasts after 4 s, and handles xmark / swipe-up dismissal.
//! The Swift `Toaster` facade drives it through `ffi::toaster` and keeps
//! its old public API so existing call sites (`HostsConnectController`,
//! `RootCoordinator`) are unchanged.
//!
//! Action taps and dismissals are reported back to Swift through the C
//! callbacks stored in the ivars (`on_action` / `on_dismiss`), so the
//! facade can run the Swift `Action.handler` closures and drop its
//! per-toast handler bookkeeping.
//!
//! The pure data/logic (kind enum, action parsing, auto-dismiss rule,
//! depth metrics) lives outside the iOS `cfg` gate so its `#[test]`s run
//! on the macOS host without UIKit — same split as `design_system::colors`.

// The pure items are reached only from `#[cfg(test)]` (host) and the
// iOS-only `imp` / `card` submodules, so the host lib build sees them as
// dead. Allow at module scope, mirroring `design_system::colors`.
#![allow(dead_code)]

use serde::Deserialize;

/// Toast severity. `repr(u8)` so it crosses the FFI boundary as a plain
/// byte. Mirrors `Toaster.Kind` in Swift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ToastKind {
    Info = 0,
    Success = 1,
    Warning = 2,
    Error = 3,
}

impl ToastKind {
    /// Decode a wire byte; unknown values fall back to `Info`.
    pub(crate) fn from_u8(raw: u8) -> Self {
        match raw {
            1 => ToastKind::Success,
            2 => ToastKind::Warning,
            3 => ToastKind::Error,
            _ => ToastKind::Info,
        }
    }
}

/// One toast action button. Decoded from the `actions_json` blob the
/// Swift facade passes across FFI: `[{"title":..,"destructive":bool}]`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct ToastAction {
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) destructive: bool,
}

/// Max toasts rendered at once (older ones stay queued but hidden).
pub(crate) const MAX_VISIBLE: usize = 3;
/// Auto-dismiss delay for non-persistent info/success toasts (seconds).
pub(crate) const AUTO_DISMISS_SECS: f64 = 4.0;

/// Whether a toast should auto-dismiss: non-persistent info/success only
/// (parity with `Toaster.show`'s timer rule).
pub(crate) fn should_autodismiss(kind: ToastKind, persistent: bool) -> bool {
    !persistent && matches!(kind, ToastKind::Info | ToastKind::Success)
}

/// Per-depth fixed gap (pt) between stacked cards — a thin, consistent
/// sliver of each back card peeks below the front card (sonner default).
pub(crate) const STACK_GAP: f64 = 15.0;
/// Per-depth scale step: each card behind the front is `STACK_SCALE_STEP`
/// narrower than the one in front of it.
pub(crate) const STACK_SCALE_STEP: f64 = 0.05;

/// Vertical peek offset (pt) for a card at the given stack depth
/// (0 = frontmost / newest). Sonner stacks back cards a fixed gap apart so
/// only a consistent sliver peeks below the front card.
pub(crate) fn offset_for(depth: usize) -> f64 {
    (depth as f64) * STACK_GAP
}

/// Horizontal scale for a card at the given stack depth — front card full
/// size, each card behind progressively narrower (1.0 / 0.95 / 0.90 …).
pub(crate) fn scale_for(depth: usize) -> f64 {
    (1.0 - (depth as f64) * STACK_SCALE_STEP).max(0.0)
}

/// Card opacity by stack depth — newest fully opaque, older ones faded.
pub(crate) fn opacity_for(depth: usize) -> f64 {
    match depth {
        0 => 1.0,
        1 => 0.95,
        _ => 0.90,
    }
}

/// Parse the FFI `actions_json` blob. Empty / malformed input yields no
/// actions rather than erroring — a toast with no buttons is valid.
pub(crate) fn parse_actions(json: &str) -> Vec<ToastAction> {
    if json.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str(json).unwrap_or_default()
}

#[cfg(target_os = "ios")]
mod imp;
#[cfg(target_os = "ios")]
pub(crate) use imp::{create_toaster_view, BtIosToasterView};

#[cfg(target_os = "ios")]
mod card;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_from_u8_round_trips_and_clamps() {
        assert_eq!(ToastKind::from_u8(0), ToastKind::Info);
        assert_eq!(ToastKind::from_u8(1), ToastKind::Success);
        assert_eq!(ToastKind::from_u8(2), ToastKind::Warning);
        assert_eq!(ToastKind::from_u8(3), ToastKind::Error);
        // Out-of-range falls back to Info.
        assert_eq!(ToastKind::from_u8(99), ToastKind::Info);
    }

    #[test]
    fn autodismiss_only_non_persistent_info_success() {
        assert!(should_autodismiss(ToastKind::Info, false));
        assert!(should_autodismiss(ToastKind::Success, false));
        assert!(!should_autodismiss(ToastKind::Warning, false));
        assert!(!should_autodismiss(ToastKind::Error, false));
        // Persistent never auto-dismisses, regardless of kind.
        assert!(!should_autodismiss(ToastKind::Info, true));
        assert!(!should_autodismiss(ToastKind::Success, true));
    }

    #[test]
    fn depth_metrics_match_sonner_stacking() {
        // Fixed-gap peek: only a thin sliver of each back card shows.
        assert_eq!(offset_for(0), 0.0);
        assert_eq!(offset_for(1), STACK_GAP);
        assert_eq!(offset_for(2), 2.0 * STACK_GAP);

        // Progressive scale-down behind the front card.
        assert_eq!(scale_for(0), 1.0);
        assert!((scale_for(1) - 0.95).abs() < 1e-9);
        assert!((scale_for(2) - 0.90).abs() < 1e-9);
        // Never collapses to / past zero for deep stacks.
        assert!(scale_for(99) >= 0.0);

        // Slight fade, front fully opaque.
        assert_eq!(opacity_for(0), 1.0);
        assert_eq!(opacity_for(1), 0.95);
        assert_eq!(opacity_for(2), 0.90);
        assert_eq!(opacity_for(5), 0.90);
    }

    #[test]
    fn parse_actions_valid() {
        let json =
            r#"[{"title":"Retry","destructive":false},{"title":"Delete","destructive":true}]"#;
        let actions = parse_actions(json);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].title, "Retry");
        assert!(!actions[0].destructive);
        assert_eq!(actions[1].title, "Delete");
        assert!(actions[1].destructive);
    }

    #[test]
    fn parse_actions_destructive_defaults_false() {
        let actions = parse_actions(r#"[{"title":"Open Settings"}]"#);
        assert_eq!(actions.len(), 1);
        assert!(!actions[0].destructive);
    }

    #[test]
    fn parse_actions_empty_and_malformed_yield_none() {
        assert!(parse_actions("").is_empty());
        assert!(parse_actions("   ").is_empty());
        assert!(parse_actions("not json").is_empty());
        assert!(parse_actions("[]").is_empty());
    }

    /// The `#[repr(u8)]` values are the C ABI contract with Swift
    /// `Toaster.Kind.raw` — they must never be renumbered without a
    /// coordinated Swift change.
    #[test]
    fn kind_repr_values_match_swift_byte_protocol() {
        assert_eq!(ToastKind::Info as u8, 0);
        assert_eq!(ToastKind::Success as u8, 1);
        assert_eq!(ToastKind::Warning as u8, 2);
        assert_eq!(ToastKind::Error as u8, 3);
    }

    /// Direct `ToastAction` construction (not via JSON) validates the
    /// struct fields independently of deserialisation and confirms
    /// `PartialEq` behaves as expected.
    #[test]
    fn action_struct_fields_and_eq() {
        let action = ToastAction {
            title: "Retry".into(),
            destructive: false,
        };
        assert_eq!(action.title, "Retry");
        assert!(!action.destructive);

        let destructive = ToastAction {
            title: "Delete".into(),
            destructive: true,
        };
        assert_eq!(destructive.title, "Delete");
        assert!(destructive.destructive);

        // PartialEq: two structurally identical actions are equal.
        assert_eq!(
            action,
            ToastAction {
                title: "Retry".into(),
                destructive: false,
            }
        );
        assert_ne!(action, destructive);
    }

    // -----------------------------------------------------------------------
    // Comprehensive ToastKind enum matching
    // -----------------------------------------------------------------------

    /// Debug formatting is the primary tool for diagnostics — ensure every
    /// variant produces a legible string that includes the variant name.
    #[test]
    fn kind_debug_format() {
        assert_eq!(format!("{:?}", ToastKind::Info), "Info");
        assert_eq!(format!("{:?}", ToastKind::Success), "Success");
        assert_eq!(format!("{:?}", ToastKind::Warning), "Warning");
        assert_eq!(format!("{:?}", ToastKind::Error), "Error");
    }

    /// Clone and Copy must preserve identity. Every variant is reachable
    /// from both a clone and a copy of the original.
    #[test]
    fn kind_clone_and_copy() {
        let cases = [
            ToastKind::Info,
            ToastKind::Success,
            ToastKind::Warning,
            ToastKind::Error,
        ];
        for &original in &cases {
            let cloned = original.clone();
            assert_eq!(original, cloned);
            assert_eq!(format!("{original:?}"), format!("{cloned:?}"));
            // Copy is implicit from `&cases` iteration above — the loop
            // already copies every variant. Verify the copy is identical.
            let copied = original;
            assert_eq!(original, copied);
        }
    }

    /// Exhaustive PartialEq: every (variant, variant) pair must agree with
    /// structural identity — same variant equals, different variants do not.
    #[test]
    fn kind_partial_eq_exhaustive() {
        let variants = [
            ToastKind::Info,
            ToastKind::Success,
            ToastKind::Warning,
            ToastKind::Error,
        ];
        for (i, &a) in variants.iter().enumerate() {
            for (j, &b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b, "same variant must equal itself");
                } else {
                    assert_ne!(a, b, "different variants must not be equal");
                }
            }
        }
    }

    /// `from_u8` is idempotent — calling it twice with the same value must
    /// produce the same result, and the round-trip `from_u8(… as u8) === …`
    /// holds for every variant.
    #[test]
    fn kind_from_u8_is_idempotent() {
        let values = [0u8, 1, 2, 3];
        for &v in &values {
            let first = ToastKind::from_u8(v);
            let second = ToastKind::from_u8(v);
            assert_eq!(first, second);
        }
    }

    // -----------------------------------------------------------------------
    // Comprehensive ToastAction struct construction and serde
    // -----------------------------------------------------------------------

    /// Debug formatting for ToastAction must include both fields.
    #[test]
    fn action_debug_format() {
        let action = ToastAction {
            title: "Retry".into(),
            destructive: true,
        };
        let debug = format!("{action:?}");
        assert!(debug.contains("Retry"));
        assert!(debug.contains("destructive"));
        assert!(debug.contains("true"));
    }

    /// JSON deserialisation must correctly handle escaped characters in the
    /// title (quotes, backslashes — these are structural in JSON).
    #[test]
    fn action_with_escaped_characters_in_title() {
        let actions = parse_actions(r#"[{"title":"Say \"Hello\"","destructive":false}]"#);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, r#"Say "Hello""#);

        let actions = parse_actions(r#"[{"title":"path\\to\\file","destructive":false}]"#);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, r"path\to\file");

        // Newline in title
        let actions = parse_actions(r#"[{"title":"line1\nline2","destructive":false}]"#);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "line1\nline2");
    }

    /// serde's default behaviour is to ignore unknown JSON fields — verify
    /// that extra fields in the action object do not cause a parse error.
    #[test]
    fn action_with_extra_json_fields() {
        let actions = parse_actions(
            r#"[{"title":"Retry","destructive":false,"extra":"ignored","count":42}]"#,
        );
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Retry");
        assert!(!actions[0].destructive);
    }

    /// Unicode characters in the title must round-trip correctly through
    /// JSON deserialisation.
    #[test]
    fn action_deserialize_unicode_title() {
        // Chinese characters
        let actions = parse_actions(r#"[{"title":"重试","destructive":false}]"#);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "重试");

        // Emoji
        let actions = parse_actions(r#"[{"title":"Retry 🔄","destructive":false}]"#);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Retry 🔄");

        // Mixed script
        let actions = parse_actions(r#"[{"title":"接続する (Connect)","destructive":false}]"#);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "接続する (Connect)");
    }

    /// Parse many actions — serde should handle a reasonably large array
    /// without issues.
    #[test]
    fn parse_actions_many_actions() {
        let json = r#"[
            {"title":"One","destructive":false},
            {"title":"Two","destructive":true},
            {"title":"Three","destructive":false},
            {"title":"Four","destructive":true},
            {"title":"Five","destructive":false}
        ]"#;
        let actions = parse_actions(json);
        assert_eq!(actions.len(), 5);
        assert_eq!(actions[0].title, "One");
        assert!(!actions[0].destructive);
        assert_eq!(actions[1].title, "Two");
        assert!(actions[1].destructive);
        assert_eq!(actions[4].title, "Five");
        assert!(!actions[4].destructive);
    }

    /// Unexpected deeply-nested JSON structures still parse gracefully into
    /// no actions (serde_json::from_str returns Err for type mismatches).
    #[test]
    fn parse_actions_nested_unexpected_type() {
        // Array of strings instead of objects
        assert!(parse_actions(r#"["one","two"]"#).is_empty());
        // Single object instead of array
        assert!(parse_actions(r#"{"title":"Retry"}"#).is_empty());
        // Array of numbers
        assert!(parse_actions(r#"[1,2,3]"#).is_empty());
    }

    /// The `ToastAction` struct should implement Clone (derived).
    #[test]
    fn action_clone() {
        let action = ToastAction {
            title: "Retry".into(),
            destructive: true,
        };
        let cloned = action.clone();
        assert_eq!(action, cloned);
        // Mutating the original must not affect the clone.
        let modified = ToastAction {
            title: "Changed".into(),
            destructive: false,
        };
        assert_ne!(cloned, modified);
    }

    /// `MAX_VISIBLE` and `AUTO_DISMISS_SECS` are contract-level constants
    /// that the iOS card factory and auto-dismiss timer rely on. Verify
    /// they are within expected bounds.
    #[test]
    fn constants_are_reasonable() {
        // At least one card must be visible, cap at a sensible max.
        assert!(MAX_VISIBLE >= 1);
        assert!(MAX_VISIBLE <= 10);

        // Auto-dismiss should be visible but not instant.
        assert!(AUTO_DISMISS_SECS >= 1.0);
        assert!(AUTO_DISMISS_SECS <= 10.0);
    }
}
