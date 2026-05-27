//! Deterministic text → RGBA mapping for view1's background.

// The colour helpers are wired in `metal_view.rs`, which compiles only on iOS.
// On other targets the unit tests still exercise them, but the public fns are
// otherwise dead → allow the warnings so `cargo clippy -D warnings` stays
// green on macOS hosts.
#![cfg_attr(not(any(target_os = "ios", test)), allow(dead_code))]

/// DJB2 hash. Stable, fast, no allocations.
fn djb2(s: &str) -> u32 {
    let mut h: u32 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(u32::from(b));
    }
    h
}

/// Convert HSL (h in degrees, s/l in 0..1) to linear RGBA (0..1, alpha = 1).
fn hsl_to_rgba(h: f32, s: f32, l: f32) -> (f32, f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m, 1.0)
}

/// Hash the input text into a mid-saturation, mid-lightness colour. Empty or
/// whitespace-only input returns a neutral grey.
pub fn hash_to_rgba(text: &str) -> (f32, f32, f32, f32) {
    if text.trim().is_empty() {
        return (0.55, 0.55, 0.55, 1.0);
    }
    let h = djb2(text);
    let hue = (h % 360) as f32;
    hsl_to_rgba(hue, 0.45, 0.55)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_grey() {
        let (r, g, b, a) = hash_to_rgba("");
        assert!((r - 0.55).abs() < 1e-6);
        assert!((g - 0.55).abs() < 1e-6);
        assert!((b - 0.55).abs() < 1e-6);
        assert_eq!(a, 1.0);
    }

    #[test]
    fn whitespace_returns_grey() {
        assert_eq!(hash_to_rgba("   "), hash_to_rgba(""));
    }

    #[test]
    fn deterministic() {
        assert_eq!(hash_to_rgba("hello"), hash_to_rgba("hello"));
    }

    #[test]
    fn different_text_different_colors() {
        assert_ne!(hash_to_rgba("hello"), hash_to_rgba("world"));
    }

    #[test]
    fn rgba_in_unit_range() {
        for s in ["a", "ab", "abc", "the quick brown fox"] {
            let (r, g, b, a) = hash_to_rgba(s);
            for v in [r, g, b, a] {
                assert!((0.0..=1.0).contains(&v), "{v} out of range for {s:?}");
            }
        }
    }
}
