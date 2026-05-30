//! Spacing constants (points). Use these instead of literals when laying
//! out stack views, paddings, and edge insets so the rhythm stays
//! consistent across Rust-built VCs.
//!
//! Today only the components in `design_system::components` consume
//! these constants; the upcoming Settings (W23b) + Onboarding (W23c)
//! VCs will lean on the full ladder, so dead-code warnings are allowed
//! at module scope.

#![allow(dead_code)]

use crate::geometry::CGFloat;

pub const XS: CGFloat = 4.0;
pub const SM: CGFloat = 8.0;
pub const MD: CGFloat = 12.0;
pub const LG: CGFloat = 16.0;
pub const XL: CGFloat = 24.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_is_monotonic() {
        let ladder = [XS, SM, MD, LG, XL];
        for pair in ladder.windows(2) {
            assert!(pair[0] < pair[1], "spacing ladder broken: {pair:?}");
        }
    }
}
