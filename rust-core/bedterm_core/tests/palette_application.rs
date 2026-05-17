//! Verifies host-pushed palette is honoured by snapshot colour resolution.

use bedterm_core::term::{Palette, Rgb24, Terminal};

#[test]
fn palette_overrides_default_foreground_and_background() {
    let mut t = Terminal::new(4, 1);
    let mut palette = Palette::default();
    palette.default_fg = Rgb24 { r: 0x11, g: 0x22, b: 0x33 };
    palette.default_bg = Rgb24 { r: 0x44, g: 0x55, b: 0x66 };
    t.set_palette(palette);

    // "AB" then end-of-screen. Default fg/bg cells should adopt new defaults.
    t.feed(b"AB");
    let snap = t.snapshot();

    let cell_a = snap.cells[0];
    assert_eq!(cell_a.fg_rgba, 0x112233FF, "default fg not applied to glyph");
    assert_eq!(cell_a.bg_rgba, 0x445566FF, "default bg not applied to glyph");
}

#[test]
fn palette_overrides_indexed_red() {
    let mut t = Terminal::new(2, 1);
    let mut palette = Palette::default();
    palette.ansi[1] = Rgb24 { r: 0xAB, g: 0xCD, b: 0xEF };
    t.set_palette(palette);

    // ESC[31m sets foreground to ANSI 1 (red); "X" then reset.
    t.feed(b"\x1b[31mX\x1b[0m");
    let snap = t.snapshot();
    assert_eq!(snap.cells[0].fg_rgba, 0xABCDEFFF, "ANSI red override ignored");
}
