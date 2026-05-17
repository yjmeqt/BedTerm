//! Thin facade over `alacritty_terminal::Term`. Owns the terminal state and an
//! event-sink that swallows everything (we don't need bell, title, clipboard
//! events at this layer — the Swift side polls snapshot + damage instead).

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::Config;
use alacritty_terminal::vte::ansi::Processor;
use alacritty_terminal::Term;

use crate::snapshot::{CellSnapshot, GridSnapshot};

/// Minimal Dimensions implementation backed by col/row counts.
#[derive(Clone, Copy, Debug)]
struct Dims {
    cols: u16,
    rows: u16,
}

impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

pub struct Terminal {
    parser: Processor,
    term: Term<VoidListener>,
    cols: u16,
    rows: u16,
}

impl Terminal {
    pub fn new(cols: u16, rows: u16) -> Self {
        let dims = Dims { cols, rows };
        let term = Term::new(Config::default(), &dims, VoidListener);
        Self {
            parser: Processor::new(),
            term,
            cols,
            rows,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let dims = Dims { cols, rows };
        self.term.resize(dims);
        self.cols = cols;
        self.rows = rows;
    }

    pub fn snapshot(&self) -> GridSnapshot {
        let cols = self.cols;
        let rows = self.rows;
        let mut cells = Vec::with_capacity(cols as usize * rows as usize);
        let grid = self.term.grid();

        for row in 0..rows as i32 {
            for col in 0..cols as usize {
                let cell = &grid[Line(row)][Column(col)];
                let f = cell.flags;
                let mut flags: u16 = 0;
                if f.contains(CellFlags::BOLD) {
                    flags |= 1;
                }
                if f.intersects(CellFlags::ALL_UNDERLINES) {
                    flags |= 2;
                }
                if f.contains(CellFlags::INVERSE) {
                    flags |= 4;
                }
                if f.contains(CellFlags::ITALIC) {
                    flags |= 8;
                }
                if f.contains(CellFlags::WIDE_CHAR) {
                    flags |= 16;
                }
                if f.contains(CellFlags::WIDE_CHAR_SPACER) {
                    flags |= 32;
                }

                // Blank cells have ' ' as their char — emit 0 for those.
                let ch = if cell.c == ' ' && f.is_empty() {
                    0
                } else {
                    cell.c as u32
                };

                cells.push(CellSnapshot {
                    ch,
                    fg_rgba: color_to_rgba(cell.fg, &self.term),
                    bg_rgba: color_to_rgba(cell.bg, &self.term),
                    flags,
                });
            }
        }

        let cursor = grid.cursor.point;
        GridSnapshot {
            cols,
            rows,
            cursor_col: cursor.column.0 as u16,
            cursor_row: cursor.line.0.max(0) as u16,
            cells,
        }
    }
}

fn color_to_rgba(c: alacritty_terminal::vte::ansi::Color, term: &Term<VoidListener>) -> u32 {
    use alacritty_terminal::vte::ansi::Color;
    let rgb = match c {
        Color::Spec(rgb) => rgb,
        Color::Named(named) => term.colors()[named].unwrap_or_else(|| default_named(named)),
        Color::Indexed(i) => term.colors()[i as usize].unwrap_or_else(|| default_indexed(i)),
    };
    ((rgb.r as u32) << 24) | ((rgb.g as u32) << 16) | ((rgb.b as u32) << 8) | 0xFF
}

fn default_named(
    n: alacritty_terminal::vte::ansi::NamedColor,
) -> alacritty_terminal::vte::ansi::Rgb {
    use alacritty_terminal::vte::ansi::{NamedColor::*, Rgb};
    match n {
        Black | DimBlack => Rgb {
            r: 0x00,
            g: 0x00,
            b: 0x00,
        },
        Red | DimRed => Rgb {
            r: 0xCC,
            g: 0x33,
            b: 0x33,
        },
        Green | DimGreen => Rgb {
            r: 0x33,
            g: 0xCC,
            b: 0x33,
        },
        Yellow | DimYellow => Rgb {
            r: 0xCC,
            g: 0xCC,
            b: 0x33,
        },
        Blue | DimBlue => Rgb {
            r: 0x33,
            g: 0x66,
            b: 0xCC,
        },
        Magenta | DimMagenta => Rgb {
            r: 0xCC,
            g: 0x33,
            b: 0xCC,
        },
        Cyan | DimCyan => Rgb {
            r: 0x33,
            g: 0xCC,
            b: 0xCC,
        },
        White | DimWhite => Rgb {
            r: 0xCC,
            g: 0xCC,
            b: 0xCC,
        },
        BrightBlack => Rgb {
            r: 0x55,
            g: 0x55,
            b: 0x55,
        },
        BrightRed => Rgb {
            r: 0xFF,
            g: 0x55,
            b: 0x55,
        },
        BrightGreen => Rgb {
            r: 0x55,
            g: 0xFF,
            b: 0x55,
        },
        BrightYellow => Rgb {
            r: 0xFF,
            g: 0xFF,
            b: 0x55,
        },
        BrightBlue => Rgb {
            r: 0x55,
            g: 0x55,
            b: 0xFF,
        },
        BrightMagenta => Rgb {
            r: 0xFF,
            g: 0x55,
            b: 0xFF,
        },
        BrightCyan => Rgb {
            r: 0x55,
            g: 0xFF,
            b: 0xFF,
        },
        BrightWhite => Rgb {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
        // Foreground defaults to a light colour; Background to black.
        Foreground | BrightForeground | DimForeground => Rgb {
            r: 0xCC,
            g: 0xCC,
            b: 0xCC,
        },
        Background => Rgb {
            r: 0x00,
            g: 0x00,
            b: 0x00,
        },
        // Cursor and anything else — white-ish.
        _ => Rgb {
            r: 0xCC,
            g: 0xCC,
            b: 0xCC,
        },
    }
}

/// Fallback for indexed colors not in the terminal palette.
/// The first 16 indices map to named colors; 16–255 are the 6×6×6 colour cube
/// and greyscale ramp — approximate them rather than produce black.
fn default_indexed(i: u8) -> alacritty_terminal::vte::ansi::Rgb {
    use alacritty_terminal::vte::ansi::{NamedColor, Rgb};
    if i < 16 {
        // SAFETY: NamedColor is repr(usize) with values 0..15 being the standard 16 colors.
        let named = match i {
            0 => NamedColor::Black,
            1 => NamedColor::Red,
            2 => NamedColor::Green,
            3 => NamedColor::Yellow,
            4 => NamedColor::Blue,
            5 => NamedColor::Magenta,
            6 => NamedColor::Cyan,
            7 => NamedColor::White,
            8 => NamedColor::BrightBlack,
            9 => NamedColor::BrightRed,
            10 => NamedColor::BrightGreen,
            11 => NamedColor::BrightYellow,
            12 => NamedColor::BrightBlue,
            13 => NamedColor::BrightMagenta,
            14 => NamedColor::BrightCyan,
            _ => NamedColor::BrightWhite,
        };
        return default_named(named);
    }
    if i < 232 {
        // 6×6×6 colour cube.
        let idx = i - 16;
        let b_idx = idx % 6;
        let g_idx = (idx / 6) % 6;
        let r_idx = idx / 36;
        let scale = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
        return Rgb {
            r: scale(r_idx),
            g: scale(g_idx),
            b: scale(b_idx),
        };
    }
    // Greyscale ramp 232–255.
    let level = 8 + (i - 232) * 10;
    Rgb {
        r: level,
        g: level,
        b: level,
    }
}
