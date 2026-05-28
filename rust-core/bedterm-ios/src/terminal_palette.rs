//! Terminal palette tokens — 16-entry ANSI palette + foreground/background.
//!
//! Port of `BedTermKit/Sources/BedTermKit/Features/Terminal/Metal/TerminalPalette.swift`.
//! Every colour reaches the Metal renderer through here — no hex literals in
//! code. Sourced from `Tokens.xcassets` named colour sets:
//! `TerminalForeground`, `TerminalBackground`, `TerminalAnsi0` … `TerminalAnsi15`.
//!
//! Strategy: asset-catalog driven (Swift's `resolve(for:)` looks up the same
//! named colours in the BedTermKit resource bundle).

// Not wired into the VC yet — this wave only exports the helpers; later waves
// will consume them (see task brief). Clippy runs with `-D warnings`, so
// silence dead-code in the meantime.
#![allow(dead_code)]

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::ClassType;
use objc2_ui_kit::UIColor;

use crate::tokens::token_color;

/// Asset-catalog token names for the 16 ANSI slots, indexed 0..15.
const ANSI_TOKEN_NAMES: [&str; 16] = [
    "TerminalAnsi0",
    "TerminalAnsi1",
    "TerminalAnsi2",
    "TerminalAnsi3",
    "TerminalAnsi4",
    "TerminalAnsi5",
    "TerminalAnsi6",
    "TerminalAnsi7",
    "TerminalAnsi8",
    "TerminalAnsi9",
    "TerminalAnsi10",
    "TerminalAnsi11",
    "TerminalAnsi12",
    "TerminalAnsi13",
    "TerminalAnsi14",
    "TerminalAnsi15",
];

/// Defensive fallback when the BedTermKit bundle / asset can't be resolved.
/// The catalogue ships with every release build, so this is purely belt-and-
/// braces (same posture as `action_chip.rs`).
fn label_color() -> Retained<UIColor> {
    unsafe { msg_send![UIColor::class(), labelColor] }
}

fn system_background_color() -> Retained<UIColor> {
    unsafe { msg_send![UIColor::class(), systemBackgroundColor] }
}

/// Resolve the ANSI palette entry for `slot` (0..=15).
///
/// # Panics
/// Panics if `slot > 15` — the ANSI palette is exactly 16 entries.
pub fn ansi(slot: u8) -> Retained<UIColor> {
    assert!(slot < 16, "ANSI palette slot out of range: {slot}");
    let name = ANSI_TOKEN_NAMES[slot as usize];
    // Sensible per-slot fallback: bright slots fall back to labelColor,
    // dark slots to systemBackgroundColor. In practice the asset catalogue
    // always resolves, so this only matters when the bundle is missing.
    let fallback = if slot >= 8 {
        label_color()
    } else {
        system_background_color()
    };
    token_color(name, fallback)
}

/// Default terminal foreground colour (`TerminalForeground`).
pub fn foreground() -> Retained<UIColor> {
    token_color("TerminalForeground", label_color())
}

/// Default terminal background colour (`TerminalBackground`).
pub fn background() -> Retained<UIColor> {
    token_color("TerminalBackground", system_background_color())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "ANSI palette slot out of range")]
    fn ansi_rejects_out_of_range() {
        let _ = ansi(16);
    }
}
