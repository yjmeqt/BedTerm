//! Code-defined dynamic UIColors that replace the `Tokens.xcassets`
//! `Shadcn*` asset-catalog lookups previously done by `tokens::token_color`.
//!
//! Each token is described by a light + dark `Rgba` pair (sRGB, 0..=1).
//! On iOS, the runtime resolves the appropriate variant through
//! `+[UIColor colorWithDynamicProvider:]`: the block we hand UIKit inspects
//! the trait collection's `userInterfaceStyle` at draw time and returns the
//! matching solid `UIColor`.
//!
//! The `Rgba` struct + the `TOKEN_TABLE` data live outside the iOS cfg gate
//! so the data-parity unit tests run on the macOS host (no UIKit symbols
//! exercised). The `Retained<UIColor>`-returning accessors are iOS-only.
//!
//! Threading: every accessor returns a `Retained<UIColor>` clone of a
//! thread-local `OnceCell`-cached dynamic color (each thread that asks
//! pays the build cost once; in practice only the main UI thread asks).
//! UIKit's contract is main-thread only, same as before.

// The `Rgba` type and `TOKEN_TABLE` data are reachable only from
// `#[cfg(test)]` modules on macOS hosts and from the iOS-only `ios`
// submodule below — both of which the dead-code lint can't see when
// compiling for host without `--tests`. Allow the warning at module
// scope.
#![allow(dead_code)]

/// Linear sRGB component triple plus alpha. All channels are in 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    #[inline]
    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }
}

/// Token identity for cross-language verification + unit tests.
///
/// Names mirror the `Tokens.xcassets` color sets they replaced. Values are
/// the exact light/dark pairs from each `.colorset/Contents.json` (sRGB,
/// alpha last).
pub const TOKEN_TABLE: &[(&str, Rgba, Rgba)] = &[
    (
        "ShadcnPrimary",
        Rgba::new(0.043, 0.043, 0.043, 1.0),
        Rgba::new(0.961, 0.961, 0.961, 1.0),
    ),
    (
        "ShadcnPrimaryForeground",
        Rgba::new(0.961, 0.961, 0.961, 1.0),
        Rgba::new(0.043, 0.043, 0.043, 1.0),
    ),
    (
        "ShadcnBackground",
        Rgba::new(0.961, 0.961, 0.961, 1.0),
        Rgba::new(0.043, 0.043, 0.043, 1.0),
    ),
    (
        "ShadcnBorder",
        Rgba::new(0.898, 0.898, 0.898, 1.0),
        Rgba::new(1.0, 1.0, 1.0, 0.10),
    ),
    (
        "ShadcnInput",
        Rgba::new(0.898, 0.898, 0.898, 1.0),
        Rgba::new(1.0, 1.0, 1.0, 0.15),
    ),
    (
        "ShadcnMutedForeground",
        Rgba::new(0.451, 0.451, 0.451, 1.0),
        Rgba::new(0.631, 0.631, 0.631, 1.0),
    ),
    (
        "ShadcnDestructive",
        Rgba::new(0.875, 0.133, 0.145, 1.0),
        Rgba::new(1.0, 0.396, 0.408, 1.0),
    ),
    (
        "ShadcnCard",
        Rgba::new(1.0, 1.0, 1.0, 1.0),
        Rgba::new(0.086, 0.086, 0.086, 1.0),
    ),
];

/// Look up a token by name in the static table. Returns `(light, dark)`.
/// Pure-data helper; host-testable.
pub fn rgba_pair(name: &str) -> Option<(Rgba, Rgba)> {
    TOKEN_TABLE
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, l, d)| (*l, *d))
}

// ---- iOS-only: dynamic-color construction ----------------------------------

#[cfg(target_os = "ios")]
pub use ios::*;

#[cfg(target_os = "ios")]
mod ios {
    use super::Rgba;
    use block2::{RcBlock, StackBlock};
    use objc2::rc::Retained;
    use objc2_ui_kit::{UIColor, UITraitCollection, UIUserInterfaceStyle};
    use std::cell::OnceCell;
    use std::ptr::NonNull;

    fn solid(rgba: Rgba) -> Retained<UIColor> {
        UIColor::colorWithRed_green_blue_alpha(rgba.r, rgba.g, rgba.b, rgba.a)
    }

    /// Build a dynamic UIColor that resolves at draw time based on the
    /// trait collection's `userInterfaceStyle`.
    ///
    /// The light + dark solid `UIColor` instances are constructed once and
    /// held by a `OnceLock` for the process lifetime (one per token, of
    /// which there are <16). The dynamic-provider block captures raw
    /// pointers to them and returns those at draw time — no per-call
    /// allocation, and the pointers stay valid forever, satisfying the
    /// "block return must be a valid pointer" contract on
    /// `colorWithDynamicProvider:`.
    fn make_dynamic(light: Rgba, dark: Rgba) -> Retained<UIColor> {
        let light_color = solid(light);
        let dark_color = solid(dark);
        // Leak the +1 retain into a `static`-lifetime pointer; the cached
        // dynamic-color accessors keep a `Retained<UIColor>` to the
        // wrapper, so these inner solids are reachable for the program
        // lifetime in the same OnceLock cache the dynamic color sits in.
        // We deliberately hold a `Retained` clone in the closure too, so
        // the inner solids cannot be released while the dynamic color is
        // alive (independent of whether the outer cache is dropped).
        let light_keep = light_color.clone();
        let dark_keep = dark_color.clone();
        let block = StackBlock::new(
            move |traits: NonNull<UITraitCollection>| -> NonNull<UIColor> {
                let style: UIUserInterfaceStyle = unsafe { traits.as_ref().userInterfaceStyle() };
                let chosen: &UIColor = if style == UIUserInterfaceStyle::Dark {
                    &dark_keep
                } else {
                    &light_keep
                };
                NonNull::from(chosen)
            },
        );
        let block: RcBlock<dyn Fn(NonNull<UITraitCollection>) -> NonNull<UIColor>> = block.copy();
        unsafe { UIColor::colorWithDynamicProvider(&block) }
    }

    /// Macro: emit a `pub fn <ident>() -> Retained<UIColor>` backed by a
    /// thread-local `OnceCell` cache; the dynamic color is constructed on
    /// first access on each thread that asks for it. In practice the only
    /// thread that reads these is the main UI thread (UIKit's contract),
    /// so each token is built exactly once per process. Thread-local
    /// caching sidesteps the `Send + Sync` requirement that a `static`
    /// `OnceLock` would impose on `Retained<UIColor>`.
    macro_rules! token_fn {
        ($fn_name:ident, $name:literal) => {
            #[allow(dead_code)]
            pub fn $fn_name() -> Retained<UIColor> {
                thread_local! {
                    static CACHE: OnceCell<Retained<UIColor>> = const { OnceCell::new() };
                }
                CACHE.with(|cell| {
                    cell.get_or_init(|| {
                        let (light, dark) =
                            super::rgba_pair($name).expect("token must exist in TOKEN_TABLE");
                        make_dynamic(light, dark)
                    })
                    .clone()
                })
            }
        };
    }

    token_fn!(shadcn_primary, "ShadcnPrimary");
    token_fn!(shadcn_primary_foreground, "ShadcnPrimaryForeground");
    token_fn!(shadcn_background, "ShadcnBackground");
    token_fn!(shadcn_border, "ShadcnBorder");
    token_fn!(shadcn_input, "ShadcnInput");
    token_fn!(shadcn_muted_foreground, "ShadcnMutedForeground");
    token_fn!(shadcn_destructive, "ShadcnDestructive");
    // Reserved for upcoming Settings/Onboarding VCs.
    token_fn!(shadcn_card, "ShadcnCard");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Rgba table must exactly match every `Shadcn*.colorset` we ported,
    /// independent of whether iOS bindings link or not.
    #[test]
    fn token_table_has_expected_entries() {
        let names: Vec<&str> = TOKEN_TABLE.iter().map(|(n, _, _)| *n).collect();
        for expected in [
            "ShadcnPrimary",
            "ShadcnPrimaryForeground",
            "ShadcnBackground",
            "ShadcnBorder",
            "ShadcnInput",
            "ShadcnMutedForeground",
            "ShadcnDestructive",
            "ShadcnCard",
        ] {
            assert!(names.contains(&expected), "missing token: {expected}");
        }
    }

    #[test]
    fn shadcn_primary_rgba_round_trip() {
        let (light, dark) = rgba_pair("ShadcnPrimary").unwrap();
        assert_eq!(light, Rgba::new(0.043, 0.043, 0.043, 1.0));
        assert_eq!(dark, Rgba::new(0.961, 0.961, 0.961, 1.0));
    }

    #[test]
    fn shadcn_border_dark_is_translucent() {
        let (_, dark) = rgba_pair("ShadcnBorder").unwrap();
        assert!(
            (dark.a - 0.10).abs() < 1e-6,
            "border dark alpha = {} (expected 0.10)",
            dark.a
        );
    }

    #[test]
    fn shadcn_input_dark_is_translucent() {
        let (_, dark) = rgba_pair("ShadcnInput").unwrap();
        assert!(
            (dark.a - 0.15).abs() < 1e-6,
            "input dark alpha = {} (expected 0.15)",
            dark.a
        );
    }

    #[test]
    fn shadcn_destructive_pair() {
        let (light, dark) = rgba_pair("ShadcnDestructive").unwrap();
        assert_eq!(light, Rgba::new(0.875, 0.133, 0.145, 1.0));
        assert_eq!(dark, Rgba::new(1.0, 0.396, 0.408, 1.0));
    }

    #[test]
    fn unknown_token_returns_none() {
        assert!(rgba_pair("ShadcnNotARealToken").is_none());
    }
}
