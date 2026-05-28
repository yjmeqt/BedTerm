//! Verifies that the four xterm-color SGR families round-trip end-to-end:
//! shell bytes → alacritty parse → `BtCellColor` → palette resolve → RGBA.

use bedterm_core::term::{BtRgb24, Palette, Terminal};

fn fresh(payload: &[u8]) -> bedterm_core::snapshot::GridSnapshot {
    let mut t = Terminal::new(8, 1);
    // Force a known palette so ansi[1] (red) etc. have predictable RGBA.
    let mut ansi = Palette::default().ansi;
    ansi[1] = BtRgb24 {
        r: 0xAA,
        g: 0x00,
        b: 0x00,
    }; // dark red
    ansi[9] = BtRgb24 {
        r: 0xFF,
        g: 0x55,
        b: 0x55,
    }; // bright red
    t.set_palette(Palette {
        default_fg: BtRgb24 {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        },
        default_bg: BtRgb24 {
            r: 0xAB,
            g: 0xCD,
            b: 0xEF,
        },
        ansi,
    });
    t.feed(payload);
    t.snapshot()
}

#[test]
fn ansi_16_color_basic_fg() {
    // ESC[31m → Named(Red) → palette.ansi[1]
    let snap = fresh(b"\x1b[31mX\x1b[0m");
    assert_eq!(snap.cells[0].fg_rgba, 0xAA00_00FF, "basic ANSI red fg");
}

#[test]
fn ansi_16_color_bright_fg() {
    // ESC[91m → Named(BrightRed) → palette.ansi[9]
    let snap = fresh(b"\x1b[91mX\x1b[0m");
    assert_eq!(snap.cells[0].fg_rgba, 0xFF55_55FF, "bright ANSI red fg");
}

#[test]
fn ansi_16_color_basic_bg() {
    // ESC[41m → bg = ansi[1]
    let snap = fresh(b"\x1b[41mX\x1b[0m");
    assert_eq!(snap.cells[0].bg_rgba, 0xAA00_00FF, "basic ANSI red bg");
}

#[test]
fn xterm_256_palette_slot_overlaps_ansi() {
    // ESC[38;5;1m → Indexed(1). Per spec, 0..15 maps onto the live ANSI
    // palette, so swapping the theme MUST flip this cell's resolved fg.
    let snap = fresh(b"\x1b[38;5;1mX\x1b[0m");
    assert_eq!(snap.cells[0].fg_rgba, 0xAA00_00FF);
}

#[test]
fn xterm_256_color_cube_is_a_protocol_constant() {
    // ESC[38;5;82m → Indexed(82). 82 - 16 = 66 = (1, 5, 0) in 6×6×6,
    // levels = [0, 95, 135, 175, 215, 255] → (95, 255, 0) = 0x5FFF00.
    let snap = fresh(b"\x1b[38;5;82mX\x1b[0m");
    assert_eq!(snap.cells[0].fg_rgba, 0x5FFF_00FF);
}

#[test]
fn xterm_256_greyscale_ramp() {
    // ESC[38;5;240m → Indexed(240). 240 - 232 = 8 ⇒ level = 8 + 8*10 = 88
    // ⇒ #585858.
    let snap = fresh(b"\x1b[38;5;240mX\x1b[0m");
    assert_eq!(snap.cells[0].fg_rgba, 0x5858_58FF);
}

#[test]
fn truecolor_24bit_fg() {
    // ESC[38;2;255;128;0m → Spec(255,128,0) → literal RGB.
    let snap = fresh(b"\x1b[38;2;255;128;0mX\x1b[0m");
    assert_eq!(snap.cells[0].fg_rgba, 0xFF80_00FF);
}

#[test]
fn truecolor_24bit_bg() {
    let snap = fresh(b"\x1b[48;2;10;20;30mX\x1b[0m");
    assert_eq!(snap.cells[0].bg_rgba, 0x0A14_1EFF);
}

#[test]
fn default_fg_bg_track_palette() {
    // No SGR — bytes inherit Foreground/Background named colors,
    // which resolve through palette.default_fg / default_bg.
    let snap = fresh(b"X");
    assert_eq!(
        snap.cells[0].fg_rgba, 0x1234_56FF,
        "default fg from palette"
    );
    assert_eq!(
        snap.cells[0].bg_rgba, 0xABCD_EFFF,
        "default bg from palette"
    );
}

#[test]
fn sgr_39_49_reset_back_to_default() {
    // ESC[31m makes fg red, ESC[39m resets fg to default.
    let snap = fresh(b"\x1b[31m\x1b[39mX");
    assert_eq!(snap.cells[0].fg_rgba, 0x1234_56FF, "SGR 39 reset");
}
