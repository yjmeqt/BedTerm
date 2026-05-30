//! Design-system colour tokens — light + dark `Rgba` pairs.
//!
//! Each token is a named `const` pair. The iOS UI layer imports these and
//! wraps them in `+[UIColor colorWithDynamicProvider:]` at first use.

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

    pub const fn light_dark(light: Self, dark: Self) -> TokenPair {
        TokenPair { light, dark }
    }
}

/// A light + dark colour pair for one design token.
pub struct TokenPair {
    pub light: Rgba,
    pub dark: Rgba,
}

// ── Tokens ──────────────────────────────────────────────────────────────

pub const PRIMARY: TokenPair = TokenPair {
    light: Rgba::new(0.043, 0.043, 0.043, 1.0),
    dark: Rgba::new(0.961, 0.961, 0.961, 1.0),
};

pub const PRIMARY_FOREGROUND: TokenPair = TokenPair {
    light: Rgba::new(0.961, 0.961, 0.961, 1.0),
    dark: Rgba::new(0.043, 0.043, 0.043, 1.0),
};

pub const BACKGROUND: TokenPair = TokenPair {
    light: Rgba::new(0.961, 0.961, 0.961, 1.0),
    dark: Rgba::new(0.043, 0.043, 0.043, 1.0),
};

pub const BORDER: TokenPair = TokenPair {
    light: Rgba::new(0.898, 0.898, 0.898, 1.0),
    dark: Rgba::new(1.0, 1.0, 1.0, 0.10),
};

pub const INPUT: TokenPair = TokenPair {
    light: Rgba::new(0.898, 0.898, 0.898, 1.0),
    dark: Rgba::new(1.0, 1.0, 1.0, 0.15),
};

pub const MUTED_FOREGROUND: TokenPair = TokenPair {
    light: Rgba::new(0.451, 0.451, 0.451, 1.0),
    dark: Rgba::new(0.631, 0.631, 0.631, 1.0),
};

pub const DESTRUCTIVE: TokenPair = TokenPair {
    light: Rgba::new(0.875, 0.133, 0.145, 1.0),
    dark: Rgba::new(1.0, 0.396, 0.408, 1.0),
};

pub const CARD: TokenPair = TokenPair {
    light: Rgba::new(1.0, 1.0, 1.0, 1.0),
    dark: Rgba::new(0.086, 0.086, 0.086, 1.0),
};

/// All tokens in declaration order — used for exhaustiveness tests.
pub const ALL_TOKENS: &[(&str, TokenPair)] = &[
    ("Primary", PRIMARY),
    ("PrimaryForeground", PRIMARY_FOREGROUND),
    ("Background", BACKGROUND),
    ("Border", BORDER),
    ("Input", INPUT),
    ("MutedForeground", MUTED_FOREGROUND),
    ("Destructive", DESTRUCTIVE),
    ("Card", CARD),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_light_is_near_black() {
        assert!((PRIMARY.light.r - 0.043).abs() < 1e-6);
        assert!((PRIMARY.light.g - 0.043).abs() < 1e-6);
        assert!((PRIMARY.light.b - 0.043).abs() < 1e-6);
    }

    #[test]
    fn primary_dark_is_near_white() {
        assert!((PRIMARY.dark.r - 0.961).abs() < 1e-6);
    }

    #[test]
    fn primary_and_foreground_are_swapped() {
        assert_eq!(PRIMARY.light, PRIMARY_FOREGROUND.dark);
        assert_eq!(PRIMARY.dark, PRIMARY_FOREGROUND.light);
    }

    #[test]
    fn background_is_primary_inverted() {
        // Primary and Background are swapped: light background = dark primary,
        // dark background = light primary.
        assert_eq!(BACKGROUND.light, PRIMARY.dark);
        assert_eq!(BACKGROUND.dark, PRIMARY.light);
    }

    #[test]
    fn border_dark_is_translucent() {
        assert!((BORDER.dark.a - 0.10).abs() < 1e-6);
    }

    #[test]
    fn input_dark_is_translucent() {
        assert!((INPUT.dark.a - 0.15).abs() < 1e-6);
    }

    #[test]
    fn destructive_pair() {
        assert_eq!(DESTRUCTIVE.light, Rgba::new(0.875, 0.133, 0.145, 1.0));
        assert_eq!(DESTRUCTIVE.dark, Rgba::new(1.0, 0.396, 0.408, 1.0));
    }

    #[test]
    fn all_tokens_has_eight_entries() {
        assert_eq!(ALL_TOKENS.len(), 8);
    }

    #[test]
    fn all_tokens_names_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for (name, _) in ALL_TOKENS {
            assert!(seen.insert(name), "duplicate token name: {name}");
        }
    }

    #[test]
    fn all_tokens_light_dark_differ() {
        for (name, pair) in ALL_TOKENS {
            assert!(
                pair.light != pair.dark,
                "token {name} has identical light/dark values"
            );
        }
    }
}
