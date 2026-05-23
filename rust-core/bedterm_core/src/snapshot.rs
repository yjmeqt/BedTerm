//! Flat grid snapshots.
//!
//! Two shapes:
//!
//! - `CellSnapshot` / `GridSnapshot` — palette-resolved (RGBA baked in).
//!   The FFI hands these to Swift, which is happy to treat the fields as
//!   final pixel colour. Used by the live PTY pane.
//! - `RawCellSnapshot` / `RawGridSnapshot` — palette-agnostic. Stores the
//!   alacritty colour intent (Named / Indexed / Spec) without flattening
//!   through any particular palette. Used for sealed blocks so a system
//!   light↔dark flip re-resolves their cells against the new palette
//!   without losing fidelity.

use crate::term::Palette;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CellSnapshot {
    /// Unicode scalar of the cell. 0 = blank. Wide-char trailing halves use char 0 with WIDE_TRAILING flag.
    pub ch: u32,
    /// Foreground colour, RGBA packed big-endian-style: 0xRRGGBBAA.
    pub fg_rgba: u32,
    /// Background colour, same encoding.
    pub bg_rgba: u32,
    /// Bitfield: 1=bold, 2=underline, 4=inverse, 8=italic, 16=wide_leading, 32=wide_trailing.
    pub flags: u16,
}

#[derive(Debug, Clone)]
pub struct GridSnapshot {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    /// Visible row of the cursor. Equals `rows` (one past last viewport row)
    /// when the cursor is outside the visible viewport because the user
    /// scrolled into history — Swift uses this as a "hide cursor" sentinel.
    pub cursor_row: u16,
    /// Current scroll position. 0 = at live bottom; positive = N rows up
    /// into the scrollback. Bounded by alacritty's history depth.
    pub display_offset: u32,
    pub cells: Vec<CellSnapshot>,
}

impl GridSnapshot {
    pub fn cell(&self, col: u16, row: u16) -> Option<&CellSnapshot> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        self.cells
            .get(row as usize * self.cols as usize + col as usize)
    }
}

/// Palette-agnostic cell colour. The renderer flattens it to RGBA via
/// `resolve` using whatever `Palette` the host has currently pushed in
/// — light↔dark flips repaint without losing any cell's original
/// colour intent.
///
/// Layout intentionally fits in 4 bytes (matches the old `u32` field
/// width inside `RawCellSnapshot`):
///
/// - `kind = 0` (Named):   `v0` is a stable `NamedSlot` byte (see below).
/// - `kind = 1` (Indexed): `v0` is the 0..=255 ANSI / 256-color index.
/// - `kind = 2` (Spec):    `(v0, v1, v2)` is the literal RGB triple.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BtCellColor {
    pub kind: u8,
    pub v0: u8,
    pub v1: u8,
    pub v2: u8,
}

/// Stable byte encoding of the alacritty `NamedColor` subset we care
/// about. Kept as plain `u8` so the C ABI stays straightforward; collapsed
/// at the convert step so the renderer's resolve function only has 18
/// branches.
pub mod named_slot {
    pub const FOREGROUND: u8 = 0;
    pub const BACKGROUND: u8 = 1;
    pub const BLACK: u8 = 2;
    pub const RED: u8 = 3;
    pub const GREEN: u8 = 4;
    pub const YELLOW: u8 = 5;
    pub const BLUE: u8 = 6;
    pub const MAGENTA: u8 = 7;
    pub const CYAN: u8 = 8;
    pub const WHITE: u8 = 9;
    pub const BRIGHT_BLACK: u8 = 10;
    pub const BRIGHT_RED: u8 = 11;
    pub const BRIGHT_GREEN: u8 = 12;
    pub const BRIGHT_YELLOW: u8 = 13;
    pub const BRIGHT_BLUE: u8 = 14;
    pub const BRIGHT_MAGENTA: u8 = 15;
    pub const BRIGHT_CYAN: u8 = 16;
    pub const BRIGHT_WHITE: u8 = 17;
}

impl BtCellColor {
    pub const fn named(slot: u8) -> Self {
        Self {
            kind: 0,
            v0: slot,
            v1: 0,
            v2: 0,
        }
    }

    pub const fn indexed(i: u8) -> Self {
        Self {
            kind: 1,
            v0: i,
            v1: 0,
            v2: 0,
        }
    }

    pub const fn spec(r: u8, g: u8, b: u8) -> Self {
        Self {
            kind: 2,
            v0: r,
            v1: g,
            v2: b,
        }
    }

    /// Flatten to `0xRRGGBBAA` using the supplied palette. Lives next to
    /// the type so renderer-side resolution doesn't have to import
    /// palette internals.
    pub fn resolve(self, palette: &Palette) -> u32 {
        let rgb = match self.kind {
            0 => named_from_slot(self.v0, palette),
            1 => {
                let i = self.v0;
                if (i as usize) < 16 {
                    palette.ansi[i as usize]
                } else {
                    let rgb = crate::term::default_indexed(i);
                    crate::term::BtRgb24 {
                        r: rgb.r,
                        g: rgb.g,
                        b: rgb.b,
                    }
                }
            }
            2 => crate::term::BtRgb24 {
                r: self.v0,
                g: self.v1,
                b: self.v2,
            },
            _ => palette.default_fg,
        };
        ((rgb.r as u32) << 24) | ((rgb.g as u32) << 16) | ((rgb.b as u32) << 8) | 0xFF
    }
}

fn named_from_slot(slot: u8, p: &Palette) -> crate::term::BtRgb24 {
    use named_slot::*;
    match slot {
        FOREGROUND => p.default_fg,
        BACKGROUND => p.default_bg,
        BLACK => p.ansi[0],
        RED => p.ansi[1],
        GREEN => p.ansi[2],
        YELLOW => p.ansi[3],
        BLUE => p.ansi[4],
        MAGENTA => p.ansi[5],
        CYAN => p.ansi[6],
        WHITE => p.ansi[7],
        BRIGHT_BLACK => p.ansi[8],
        BRIGHT_RED => p.ansi[9],
        BRIGHT_GREEN => p.ansi[10],
        BRIGHT_YELLOW => p.ansi[11],
        BRIGHT_BLUE => p.ansi[12],
        BRIGHT_MAGENTA => p.ansi[13],
        BRIGHT_CYAN => p.ansi[14],
        BRIGHT_WHITE => p.ansi[15],
        _ => p.default_fg,
    }
}

/// Convert an alacritty `Color` into our palette-agnostic form. Collapses
/// `BrightForeground` / `DimForeground` / `DimBlack` etc. onto the
/// matching regular slot, mirroring the behaviour of the legacy
/// `color_to_rgba` resolver.
pub fn cell_color_from_alacritty(c: alacritty_terminal::vte::ansi::Color) -> BtCellColor {
    use alacritty_terminal::vte::ansi::{Color, NamedColor::*};
    match c {
        Color::Spec(rgb) => BtCellColor::spec(rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => BtCellColor::indexed(i),
        Color::Named(n) => {
            let slot = match n {
                Foreground | BrightForeground | DimForeground => named_slot::FOREGROUND,
                Background => named_slot::BACKGROUND,
                Black | DimBlack => named_slot::BLACK,
                Red | DimRed => named_slot::RED,
                Green | DimGreen => named_slot::GREEN,
                Yellow | DimYellow => named_slot::YELLOW,
                Blue | DimBlue => named_slot::BLUE,
                Magenta | DimMagenta => named_slot::MAGENTA,
                Cyan | DimCyan => named_slot::CYAN,
                White | DimWhite => named_slot::WHITE,
                BrightBlack => named_slot::BRIGHT_BLACK,
                BrightRed => named_slot::BRIGHT_RED,
                BrightGreen => named_slot::BRIGHT_GREEN,
                BrightYellow => named_slot::BRIGHT_YELLOW,
                BrightBlue => named_slot::BRIGHT_BLUE,
                BrightMagenta => named_slot::BRIGHT_MAGENTA,
                BrightCyan => named_slot::BRIGHT_CYAN,
                BrightWhite => named_slot::BRIGHT_WHITE,
                _ => named_slot::FOREGROUND,
            };
            BtCellColor::named(slot)
        }
    }
}

/// Palette-agnostic cell. Same fields as `CellSnapshot` except the
/// colours stay unresolved.
#[derive(Clone, Copy, Debug)]
pub struct RawCellSnapshot {
    pub ch: u32,
    pub fg: BtCellColor,
    pub bg: BtCellColor,
    pub flags: u16,
}

#[derive(Debug, Clone)]
pub struct RawGridSnapshot {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub display_offset: u32,
    pub cells: Vec<RawCellSnapshot>,
}

impl RawGridSnapshot {
    /// Allocate a palette-resolved `GridSnapshot` from `self`. Used at
    /// the FFI boundary when handing a frozen block to Swift, and when
    /// tests want plain RGBA cells back.
    pub fn resolve(&self, palette: &Palette) -> GridSnapshot {
        let cells = self
            .cells
            .iter()
            .map(|c| CellSnapshot {
                ch: c.ch,
                fg_rgba: c.fg.resolve(palette),
                bg_rgba: c.bg.resolve(palette),
                flags: c.flags,
            })
            .collect();
        GridSnapshot {
            cols: self.cols,
            rows: self.rows,
            cursor_col: self.cursor_col,
            cursor_row: self.cursor_row,
            display_offset: self.display_offset,
            cells,
        }
    }
}
