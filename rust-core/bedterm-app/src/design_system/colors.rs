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
// The `ios` submodule with `shadcn_*()` UIColor factories lives in
// `bedterm-ios/src/design_system/colors.rs` and imports `Rgba` / `rgba_pair`
// from this crate.

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
