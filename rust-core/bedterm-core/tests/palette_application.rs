//! Verifies host-pushed palette is honoured by snapshot colour resolution,
//! and that sealed blocks re-resolve through the current palette instead
//! of staying frozen at the colour they were sealed with.

use bedterm_core::term::{BtRgb24, Palette, Terminal};

/// `ESC P $ d <hex(JSON)> ESC \` — the 7-bit DCS form used by the
/// bedterm shell-integration script. Inlined here because the helper
/// in `term.rs`'s test module isn't visible across crate boundaries.
fn dcs7(json: &str) -> Vec<u8> {
    let mut v = vec![0x1B, b'P', b'$', b'd'];
    let mut hex = String::with_capacity(json.len() * 2);
    for b in json.bytes() {
        use std::fmt::Write;
        write!(hex, "{b:02x}").unwrap();
    }
    v.extend_from_slice(hex.as_bytes());
    v.extend_from_slice(b"\x1b\\");
    v
}

/// Drive one full Preexec → output → CommandFinished cycle into `t`,
/// leaving exactly one sealed block whose body has `payload` on row 0.
fn seal_one_block(t: &mut Terminal, payload: &[u8]) {
    t.feed(&dcs7(r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#));
    t.feed(&dcs7(r#"{"hook":"Preexec","value":{"command":"ls"}}"#));
    t.feed(payload);
    t.feed(&dcs7(
        r#"{"hook":"CommandFinished","value":{"exit_code":0}}"#,
    ));
}

#[test]
fn palette_overrides_default_foreground_and_background() {
    let mut t = Terminal::new(4, 1);
    let palette = Palette {
        default_fg: BtRgb24 {
            r: 0x11,
            g: 0x22,
            b: 0x33,
        },
        default_bg: BtRgb24 {
            r: 0x44,
            g: 0x55,
            b: 0x66,
        },
        ..Palette::default()
    };
    t.set_palette(palette);

    // "AB" — every glyph cell whose fg/bg resolves to the Foreground/Background named colour should adopt the configured defaults.
    t.feed(b"AB");
    let snap = t.snapshot();

    let cell_a = snap.cells[0];
    assert_eq!(
        cell_a.fg_rgba, 0x112233FF,
        "default fg not applied to glyph"
    );
    assert_eq!(
        cell_a.bg_rgba, 0x445566FF,
        "default bg not applied to glyph"
    );
}

#[test]
fn palette_overrides_indexed_red() {
    let mut t = Terminal::new(2, 1);
    let mut ansi = Palette::default().ansi;
    ansi[1] = BtRgb24 {
        r: 0xAB,
        g: 0xCD,
        b: 0xEF,
    };
    let palette = Palette {
        ansi,
        ..Palette::default()
    };
    t.set_palette(palette);

    // ESC[31m sets foreground to ANSI 1 (red); "X" then reset.
    t.feed(b"\x1b[31mX\x1b[0m");
    let snap = t.snapshot();
    assert_eq!(
        snap.cells[0].fg_rgba, 0xABCDEFFF,
        "ANSI red override ignored"
    );
}

/// Seals a block in a tiny terminal, then verifies that re-resolving its
/// frozen snapshot through two different palettes (a "light" one with
/// white bg + black fg, then a "dark" one with the inverse) produces
/// different RGBA — i.e. the Option-3 design holds: cells are stored
/// palette-agnostic and follow the live palette.
#[test]
fn frozen_block_reresolves_through_current_palette() {
    let mut t = Terminal::new(8, 2);

    // DCS Precmd → Preexec → output "hi\r\n" → CommandFinished. After
    // this run, `blocks[0]` is sealed with a `RawGridSnapshot`.
    seal_one_block(&mut t, b"hi\r\n");

    let frozen = t
        .blocks()
        .first()
        .and_then(|b| b.frozen_snapshot.as_ref())
        .expect("expected one sealed block with a frozen snapshot");

    let light = Palette {
        default_fg: BtRgb24 {
            r: 0x00,
            g: 0x00,
            b: 0x00,
        },
        default_bg: BtRgb24 {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
        ..Palette::default()
    };
    let dark = Palette {
        default_fg: BtRgb24 {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
        default_bg: BtRgb24 {
            r: 0x00,
            g: 0x00,
            b: 0x00,
        },
        ..Palette::default()
    };

    let resolved_light = frozen.resolve(&light);
    let resolved_dark = frozen.resolve(&dark);

    // 'h' lives at row 0 col 0 — it was printed with the shell's default
    // fg/bg, so resolution must follow the supplied palette.
    let h_light = resolved_light.cell(0, 0).expect("h cell in light");
    let h_dark = resolved_dark.cell(0, 0).expect("h cell in dark");
    assert_eq!(
        h_light.bg_rgba, 0xFFFF_FFFF,
        "light palette should resolve default bg to white, got {:08X}",
        h_light.bg_rgba
    );
    assert_eq!(
        h_dark.bg_rgba, 0x0000_00FF,
        "dark palette should resolve default bg to black, got {:08X}",
        h_dark.bg_rgba
    );
    assert_eq!(
        h_light.fg_rgba, 0x0000_00FF,
        "light palette should resolve default fg to black, got {:08X}",
        h_light.fg_rgba
    );
    assert_eq!(
        h_dark.fg_rgba, 0xFFFF_FFFF,
        "dark palette should resolve default fg to white, got {:08X}",
        h_dark.fg_rgba
    );
    assert_ne!(
        h_light.bg_rgba, h_dark.bg_rgba,
        "frozen block did not re-resolve through palette change — \
         palette decoupling regressed"
    );

    // Sanity: the cell's printed character survived the freeze.
    assert_eq!(h_light.ch, u32::from('h'));
    assert_eq!(h_dark.ch, u32::from('h'));
}

/// End-to-end version using `Terminal::set_palette` — mirrors what the
/// host does on a system light↔dark flip. After sealing under `A`,
/// pushing palette `B` and reading the block out should yield `B`'s
/// colours without re-feeding any bytes.
#[test]
fn light_to_dark_flip_repaints_sealed_block() {
    let mut t = Terminal::new(8, 2);

    let light = Palette {
        default_fg: BtRgb24 {
            r: 0x11,
            g: 0x11,
            b: 0x11,
        },
        default_bg: BtRgb24 {
            r: 0xEE,
            g: 0xEE,
            b: 0xEE,
        },
        ..Palette::default()
    };
    t.set_palette(light);
    seal_one_block(&mut t, b"X\r\n");

    // Now flip the palette — the host's analogue of dark mode kicking in
    // while a sealed block sits in scrollback.
    let dark = Palette {
        default_fg: BtRgb24 {
            r: 0xEE,
            g: 0xEE,
            b: 0xEE,
        },
        default_bg: BtRgb24 {
            r: 0x11,
            g: 0x11,
            b: 0x11,
        },
        ..Palette::default()
    };
    t.set_palette(dark);

    let frozen = t.blocks()[0]
        .frozen_snapshot
        .as_ref()
        .expect("sealed block");
    let cell = frozen
        .resolve(t.palette())
        .cell(0, 0)
        .copied()
        .expect("first cell");
    assert_eq!(cell.bg_rgba, 0x1111_11FF, "expected dark bg post-flip");
    assert_eq!(cell.fg_rgba, 0xEEEE_EEFF, "expected dark fg post-flip");
}
