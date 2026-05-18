//! Thin facade over `alacritty_terminal::Term`. Owns the terminal state and an
//! event-sink that swallows everything (we don't need bell, title, clipboard
//! events at this layer — the Swift side polls snapshot + damage instead).

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use alacritty_terminal::Term;

use crate::snapshot::{CellSnapshot, GridSnapshot};

// Stable BedTerm-side mode bits exposed across FFI. Decoupled from alacritty's
// internal TermMode positions so an upstream upgrade can't silently shift the
// values Swift observes. Keep in sync with `BedTermMode` in Swift.
pub const BT_MODE_ALT_SCREEN: u32 = 1 << 0;
pub const BT_MODE_BRACKETED_PASTE: u32 = 1 << 1;
pub const BT_MODE_MOUSE_REPORT: u32 = 1 << 2;
pub const BT_MODE_APP_CURSOR: u32 = 1 << 3;
pub const BT_MODE_APP_KEYPAD: u32 = 1 << 4;
pub const BT_MODE_FOCUS_IN_OUT: u32 = 1 << 5;

/// 8-bit-per-channel sRGB triple. The renderer-facing snapshot stores
/// premultiplied RGBA u32s; this type only exists at the host-config boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct BtRgb24 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// 18-colour terminal palette: the 16 ANSI indices plus the two defaults.
/// The host (Swift) recomputes this from design tokens when the iOS
/// appearance changes, and pushes it through `Terminal::set_palette`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Palette {
    pub default_fg: BtRgb24,
    pub default_bg: BtRgb24,
    pub ansi: [BtRgb24; 16],
}

impl Default for Palette {
    fn default() -> Self {
        // Matches the legacy hardcoded palette (the values that previously
        // lived in `default_named`). Kept as the no-host-yet fallback.
        use alacritty_terminal::vte::ansi::NamedColor;
        let n = |c: NamedColor| -> BtRgb24 {
            let rgb = legacy_default_named(c);
            BtRgb24 {
                r: rgb.r,
                g: rgb.g,
                b: rgb.b,
            }
        };
        Self {
            default_fg: n(NamedColor::Foreground),
            default_bg: n(NamedColor::Background),
            ansi: [
                n(NamedColor::Black),
                n(NamedColor::Red),
                n(NamedColor::Green),
                n(NamedColor::Yellow),
                n(NamedColor::Blue),
                n(NamedColor::Magenta),
                n(NamedColor::Cyan),
                n(NamedColor::White),
                n(NamedColor::BrightBlack),
                n(NamedColor::BrightRed),
                n(NamedColor::BrightGreen),
                n(NamedColor::BrightYellow),
                n(NamedColor::BrightBlue),
                n(NamedColor::BrightMagenta),
                n(NamedColor::BrightCyan),
                n(NamedColor::BrightWhite),
            ],
        }
    }
}

/// Scrollback buffer size. 10 000 lines × 80 cols × ~16 B/cell ≈ 12 MB worst
/// case, on par with the glyph atlas budget.
const SCROLLBACK_LINES: u16 = 10_000;

/// Minimal Dimensions implementation. `total_lines` includes scrollback so the
/// alacritty grid actually allocates history.
#[derive(Clone, Copy, Debug)]
struct Dims {
    cols: u16,
    screen_rows: u16,
}

impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.screen_rows as usize + SCROLLBACK_LINES as usize
    }

    fn screen_lines(&self) -> usize {
        self.screen_rows as usize
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
    palette: Palette,
    osc133: crate::osc133::Osc133Sniffer,
}

impl Terminal {
    pub fn new(cols: u16, rows: u16) -> Self {
        let dims = Dims {
            cols,
            screen_rows: rows,
        };
        let term = Term::new(Config::default(), &dims, VoidListener);
        Self {
            parser: Processor::new(),
            term,
            cols,
            rows,
            palette: Palette::default(),
            osc133: crate::osc133::Osc133Sniffer::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        // Dual-feed: alacritty owns grid mutation, while the OSC 133 sniffer
        // listens for shell-integration markers. The sniffer's VTE state
        // machine is independent — both must see every byte to stay in sync.
        self.parser.advance(&mut self.term, bytes);
        self.osc133.feed(bytes);
    }

    /// Pop the next pending OSC 133 event, or `None` if the queue is empty.
    /// Cheap; safe to drain after every feed.
    pub fn pop_osc133(&mut self) -> Option<crate::osc133::Osc133Event> {
        self.osc133.pop()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let dims = Dims {
            cols,
            screen_rows: rows,
        };
        self.term.resize(dims);
        self.cols = cols;
        self.rows = rows;
    }

    /// Scroll the display by `delta` rows. Positive = into history (up),
    /// negative = toward live bottom (down). Clamped by alacritty to
    /// `[0, history_size]`.
    pub fn scroll_by(&mut self, delta: i32) {
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    pub fn scroll_offset(&self) -> u32 {
        self.term.grid().display_offset() as u32
    }

    pub fn scrollback_lines(&self) -> u32 {
        self.term.grid().history_size() as u32
    }

    /// Encode the terminal's current mode flags into BedTerm-stable bits.
    /// Returns a `u32` bitmask of `BT_MODE_*` constants.
    pub fn mode(&self) -> u32 {
        let m = *self.term.mode();
        let mut out: u32 = 0;
        if m.contains(TermMode::ALT_SCREEN) {
            out |= BT_MODE_ALT_SCREEN;
        }
        if m.contains(TermMode::BRACKETED_PASTE) {
            out |= BT_MODE_BRACKETED_PASTE;
        }
        if m.intersects(TermMode::MOUSE_MODE) {
            out |= BT_MODE_MOUSE_REPORT;
        }
        if m.contains(TermMode::APP_CURSOR) {
            out |= BT_MODE_APP_CURSOR;
        }
        if m.contains(TermMode::APP_KEYPAD) {
            out |= BT_MODE_APP_KEYPAD;
        }
        if m.contains(TermMode::FOCUS_IN_OUT) {
            out |= BT_MODE_FOCUS_IN_OUT;
        }
        out
    }

    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    /// Grid line of the cursor on the active screen (`0..rows-1`). Swift
    /// records this on each OSC 133 boundary so Block view knows where its
    /// row range starts / ends. Combined with `history_size()` at the same
    /// instant, the (line, history) pair anchors the block stably as the
    /// grid scrolls — see `snapshot_range` for the lookup math.
    pub fn current_line(&self) -> i32 {
        self.term.grid().cursor.point.line.0
    }

    /// Snapshot a row range from the active screen + scrollback. Coordinates
    /// are grid lines: `0` is the top of the active screen, negatives index
    /// into history. `start_line` inclusive, `end_line` exclusive. Lines
    /// outside the grid's current extent are clamped; if the entire range
    /// has fallen off scrollback the returned snapshot has `rows == 0`.
    pub fn snapshot_range(&self, start_line: i32, end_line: i32) -> GridSnapshot {
        let cols = self.cols;
        let grid = self.term.grid();
        let history = grid.history_size() as i32;
        let min_line = -history;
        let max_line = self.rows as i32;
        let start = start_line.max(min_line).min(max_line);
        let end = end_line.max(start).min(max_line);
        let row_count = (end - start).max(0) as u16;

        let mut cells = Vec::with_capacity(cols as usize * row_count as usize);
        for line in start..end {
            for col in 0..cols as usize {
                let cell = &grid[Line(line)][Column(col)];
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
                let ch =
                    if f.contains(CellFlags::WIDE_CHAR_SPACER) || (cell.c == ' ' && f.is_empty()) {
                        0
                    } else {
                        cell.c as u32
                    };
                cells.push(CellSnapshot {
                    ch,
                    fg_rgba: color_to_rgba(cell.fg, &self.palette),
                    bg_rgba: color_to_rgba(cell.bg, &self.palette),
                    flags,
                });
            }
        }

        // No cursor for range snapshots — Block bodies don't show one.
        // `cursor_row = rows` signals "hidden" to Swift.
        GridSnapshot {
            cols,
            rows: row_count,
            cursor_col: 0,
            cursor_row: row_count,
            display_offset: 0,
            cells,
        }
    }

    pub fn snapshot(&self) -> GridSnapshot {
        let cols = self.cols;
        let rows = self.rows;
        let mut cells = Vec::with_capacity(cols as usize * rows as usize);
        let grid = self.term.grid();
        let offset = grid.display_offset() as i32;

        for row in 0..rows as i32 {
            for col in 0..cols as usize {
                // Visible row `row` maps to grid Line(row - offset).
                // offset=0: 0..rows-1 (live screen). offset=N: -N..rows-1-N
                // (N rows of history at the top of the viewport).
                let cell = &grid[Line(row - offset)][Column(col)];
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
                // Wide-char spacers are the trailing half of a CJK glyph that
                // lives in the previous (WIDE_CHAR) cell; emit 0 so the
                // renderer skips them and the wide cell's quad covers both
                // columns without overdrawing a stray space glyph.
                let ch =
                    if f.contains(CellFlags::WIDE_CHAR_SPACER) || (cell.c == ' ' && f.is_empty()) {
                        0
                    } else {
                        cell.c as u32
                    };

                cells.push(CellSnapshot {
                    ch,
                    fg_rgba: color_to_rgba(cell.fg, &self.palette),
                    bg_rgba: color_to_rgba(cell.bg, &self.palette),
                    flags,
                });
            }
        }

        let cursor = grid.cursor.point;
        // Cursor lives at grid Line(cursor.line.0) on the live screen. Its
        // visible row when display is scrolled is cursor.line.0 + offset.
        // When the cursor is scrolled off-screen we emit `rows` (one past
        // the last visible row) as a sentinel — Swift treats this as "hide".
        let cursor_visual_row = cursor.line.0 + offset;
        let cursor_row = if (0..rows as i32).contains(&cursor_visual_row) {
            cursor_visual_row as u16
        } else {
            rows
        };

        GridSnapshot {
            cols,
            rows,
            cursor_col: cursor.column.0 as u16,
            cursor_row,
            display_offset: offset.max(0) as u32,
            cells,
        }
    }
}

fn color_to_rgba(c: alacritty_terminal::vte::ansi::Color, palette: &Palette) -> u32 {
    use alacritty_terminal::vte::ansi::Color;
    let rgb = match c {
        Color::Spec(rgb) => BtRgb24 {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
        Color::Named(named) => named_from_palette(named, palette),
        Color::Indexed(i) => {
            if (i as usize) < 16 {
                palette.ansi[i as usize]
            } else {
                // 16..=255 — colour cube and greyscale ramp, not palette-controlled.
                let rgb = default_indexed(i);
                BtRgb24 {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                }
            }
        }
    };
    ((rgb.r as u32) << 24) | ((rgb.g as u32) << 16) | ((rgb.b as u32) << 8) | 0xFF
}

fn named_from_palette(n: alacritty_terminal::vte::ansi::NamedColor, p: &Palette) -> BtRgb24 {
    use alacritty_terminal::vte::ansi::NamedColor::*;
    match n {
        Foreground | BrightForeground | DimForeground => p.default_fg,
        Background => p.default_bg,
        Black | DimBlack => p.ansi[0],
        Red | DimRed => p.ansi[1],
        Green | DimGreen => p.ansi[2],
        Yellow | DimYellow => p.ansi[3],
        Blue | DimBlue => p.ansi[4],
        Magenta | DimMagenta => p.ansi[5],
        Cyan | DimCyan => p.ansi[6],
        White | DimWhite => p.ansi[7],
        BrightBlack => p.ansi[8],
        BrightRed => p.ansi[9],
        BrightGreen => p.ansi[10],
        BrightYellow => p.ansi[11],
        BrightBlue => p.ansi[12],
        BrightMagenta => p.ansi[13],
        BrightCyan => p.ansi[14],
        BrightWhite => p.ansi[15],
        // Cursor and anything else — use default foreground. Note: BrightForeground
        // and DimForeground are matched explicitly above because the palette has no
        // distinct slot for them; they intentionally collapse onto default_fg.
        _ => p.default_fg,
    }
}

fn legacy_default_named(
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
        return legacy_default_named(named);
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
