//! Per-block VTE + grid. Mirrors Warp's `BlockGrid` — each block can own a
//! private terminal that receives only the bytes printed between its
//! `Preexec` and `CommandFinished` boundaries, so the command's cursor
//! positioning / erase-line / redraws stay scoped to the block's body
//! instead of leaking into the global terminal's prompt area.
//!
//! Conceptually a slim sibling of [`crate::term::Terminal`]: same alacritty
//! `Term<VoidListener>` + `Processor` pair, but no `BlockStore`, no DCS
//! sniffer (the parent `Terminal` handles those), no scrollback (a single
//! command's output rarely exceeds the screen, and any leftovers freeze
//! into `frozen_snapshot` at `CommandFinished`).

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::Config;
use alacritty_terminal::vte::ansi::Processor;
use alacritty_terminal::Term;

use crate::snapshot::{CellSnapshot, GridSnapshot};
use crate::term::{color_to_rgba, Dims, Palette};

pub struct BlockGrid {
    parser: Processor,
    term: Term<VoidListener>,
    cols: u16,
    rows: u16,
}

impl std::fmt::Debug for BlockGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockGrid")
            .field("cols", &self.cols)
            .field("rows", &self.rows)
            .finish_non_exhaustive()
    }
}

impl BlockGrid {
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
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.parser.advance(&mut self.term, bytes);
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

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Cursor's current row on the active screen (0..rows-1). Mirrors
    /// `Terminal::current_line` but without the history offset — a
    /// block's grid is fresh per command, so there's nothing in
    /// history to add.
    pub fn cursor_row(&self) -> i32 {
        self.term.grid().cursor.point.line.0
    }

    /// Total rows of body content this block has produced — scrollback
    /// lines that fell off the top of the visible grid PLUS the
    /// currently-occupied rows of the visible grid. A command that
    /// printed 80 lines into a 29-row PTY returns 80 (history=51 +
    /// visible_used=29); a command that printed 5 lines returns 5
    /// (history=0 + visible_used=5). Used by the host to size the
    /// block's body extent — must match the row count returned by
    /// `snapshot()`.
    pub fn used_rows(&self) -> u16 {
        let grid = self.term.grid();
        let history = (grid.total_lines() - grid.screen_lines()) as u16;
        // Find the bottom-most visible row that actually contains
        // content. Mirrors Warp's `output_grid.len_displayed()` — block
        // height shrinks to fit the printed lines instead of padding
        // out to the full PTY screen height. Without this a `claude`
        // exit (which paints ~9 lines then leaves the cursor parked
        // wherever) gives the block a 39-row body with 30 empty rows
        // and a visible gap above the composer.
        let visible_used = self.last_content_row().map_or_else(
            || ((self.cursor_row() + 1).clamp(1, self.rows as i32)) as u16,
            |r| (r as u16).saturating_add(1),
        );
        history + visible_used
    }

    /// Scan the visible grid bottom-up for the last row that has any
    /// non-blank cell (printable char or non-default background). Returns
    /// `None` if the whole visible grid is blank. The cursor's row is
    /// also considered "used" so a freshly opened block with the cursor
    /// at row 0 still claims one row.
    fn last_content_row(&self) -> Option<i32> {
        let grid = self.term.grid();
        let cols = self.cols as usize;
        let cursor_row = grid.cursor.point.line.0;
        let mut best = if (0..self.rows as i32).contains(&cursor_row) {
            Some(cursor_row)
        } else {
            None
        };
        for row in (0..self.rows as i32).rev() {
            if best.is_some_and(|b| b >= row) {
                break;
            }
            for col in 0..cols {
                let cell = &grid[Line(row)][Column(col)];
                let has_glyph = cell.c != ' ' || !cell.flags.is_empty();
                if has_glyph {
                    best = Some(row);
                    break;
                }
            }
        }
        best
    }

    /// Borrow the underlying alacritty `Term` so the renderer / FFI
    /// layer can read its grid directly. Pure read-only access; do
    /// NOT mutate via this handle (use `feed` / `resize`).
    pub fn term(&self) -> &Term<VoidListener> {
        &self.term
    }

    /// Snapshot the entire screen (rows × cols) using `palette` to
    /// resolve named colours. Same encoding as
    /// `Terminal::snapshot_range_from` so the renderer can feed both
    /// snapshot streams through a single pipeline.
    pub fn snapshot(&self, palette: &Palette) -> GridSnapshot {
        let grid = self.term.grid();
        let cols = self.cols;
        // Capture scrollback PLUS only the visible rows that actually
        // contain printed content. Without this clamp a TUI like claude
        // that paints ~9 lines and exits leaves ~30 blank rows trailing
        // the body — the host renders an empty gap below the last
        // printed line and the block visibly fails to "close up" to the
        // composer.
        let history = (grid.total_lines() - grid.screen_lines()) as i32;
        let visible_used = self.last_content_row().map_or_else(
            || (self.cursor_row() + 1).clamp(1, self.rows as i32),
            |r| r + 1,
        );
        let total_rows = (history + visible_used) as u16;
        let mut cells = Vec::with_capacity(cols as usize * total_rows as usize);
        for row in -history..visible_used {
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
                let ch =
                    if f.contains(CellFlags::WIDE_CHAR_SPACER) || (cell.c == ' ' && f.is_empty()) {
                        0
                    } else {
                        cell.c as u32
                    };
                cells.push(CellSnapshot {
                    ch,
                    fg_rgba: color_to_rgba(cell.fg, palette),
                    bg_rgba: color_to_rgba(cell.bg, palette),
                    flags,
                });
            }
        }
        let cursor = grid.cursor.point;
        // Cursor row in the snapshot's flattened coords (history rows
        // above + cursor's visible-grid row). Stays in-range as long
        // as the cursor is on the visible grid; off-screen cursors
        // collapse to the bottom.
        let cursor_row = if (0..visible_used).contains(&cursor.line.0) {
            (history + cursor.line.0) as u16
        } else {
            total_rows
        };
        GridSnapshot {
            cols,
            rows: total_rows,
            cursor_col: cursor.column.0 as u16,
            cursor_row,
            display_offset: 0,
            cells,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette::default()
    }

    #[test]
    fn writes_ascii_text() {
        let mut g = BlockGrid::new(20, 5);
        g.feed(b"hi");
        let snap = g.snapshot(&palette());
        assert_eq!(snap.cols, 20);
        // Snapshot trims trailing blank rows to match Warp's
        // `output_grid.len_displayed()`. After "hi" the cursor still
        // sits on row 0, so the body is one row tall.
        assert_eq!(snap.rows, 1);
        assert_eq!(snap.cells[0].ch, 'h' as u32);
        assert_eq!(snap.cells[1].ch, 'i' as u32);
        // cursor should be on row 0 col 2
        assert_eq!(snap.cursor_row, 0);
        assert_eq!(snap.cursor_col, 2);
    }

    #[test]
    fn handles_cursor_positioning_in_place() {
        // Write "abc\n", reposition to row 0 col 1, overwrite 'b' with 'X'.
        let mut g = BlockGrid::new(10, 4);
        g.feed(b"abc\n");
        g.feed(b"\x1b[1;2H"); // CSI 1;2 H — row 1 col 2 in 1-indexed → row 0 col 1
        g.feed(b"X");
        let snap = g.snapshot(&palette());
        assert_eq!(snap.cells[0].ch, 'a' as u32);
        assert_eq!(snap.cells[1].ch, 'X' as u32);
        assert_eq!(snap.cells[2].ch, 'c' as u32);
    }

    #[test]
    fn resize_clamps_cursor() {
        let mut g = BlockGrid::new(20, 5);
        g.feed(b"abcdefghij");
        g.resize(8, 3);
        assert_eq!(g.cols(), 8);
        assert_eq!(g.rows(), 3);
    }

    #[test]
    fn output_exceeding_visible_grid_lands_in_scrollback() {
        // 5-row grid, write 12 numbered lines. Scrollback should hold
        // the first 7 lines, visible the last 5; snapshot should
        // contain 12 rows total with "1" at top and "12" near bottom.
        let mut g = BlockGrid::new(20, 5);
        for n in 1..=12 {
            g.feed(format!("{n}\r\n").as_bytes());
        }
        // 12 lines printed + 1 empty current line = 13 used rows
        // (8 in history "1".."8", 5 on visible grid "9"-"12" + empty
        // current-cursor line).
        assert_eq!(g.used_rows(), 13);
        let snap = g.snapshot(&palette());
        assert_eq!(snap.rows, 13);
        // First cell of row 0 should be '1'.
        assert_eq!(snap.cells[0].ch, '1' as u32);
        // Row 11 should be "12"; row 12 is the empty cursor line.
        let r11_first = snap.cells[11 * 20].ch;
        let r11_second = snap.cells[11 * 20 + 1].ch;
        assert_eq!(r11_first, '1' as u32);
        assert_eq!(r11_second, '2' as u32);
    }

    #[test]
    fn carriage_return_overwrites_in_place() {
        // Mirrors `test-scripts/05-progress.sh` — progress bar prints
        // `\r[####] N%` 40 times. With CR semantics the bar should
        // overwrite the same row; the snapshot should show only the
        // final line, not 40 stacked rows.
        let mut g = BlockGrid::new(40, 5);
        for n in 1..=10 {
            let bar = "#".repeat(n);
            let line = format!("\r[{bar}] {n}%");
            g.feed(line.as_bytes());
        }
        let snap = g.snapshot(&palette());
        // First (and only) row should contain "[##########] 10%".
        let mut row0: String = String::new();
        for col in 0..snap.cols as usize {
            let ch = snap.cells[col].ch;
            if ch == 0 {
                row0.push(' ');
            } else if let Some(c) = char::from_u32(ch) {
                row0.push(c);
            }
        }
        let trimmed = row0.trim_end();
        assert_eq!(trimmed, "[##########] 10%", "row 0 got {row0:?}");
        // Snapshot trims trailing blank rows now, so if CR truly
        // overwrites in place we only see one row (no row 1 to be
        // accidentally filled). A regression to "\r treated as \n"
        // would push the snapshot past one row and this would fail.
        assert_eq!(
            snap.rows, 1,
            "expected single overwriting row, got {} rows",
            snap.rows
        );
    }

    #[test]
    fn clear_does_not_swallow_subsequent_text() {
        // Reproduces test-scripts/10-bell-clear.sh: three lines of
        // output, then `clear` (≈ ESC [H ESC [2J), then a final line.
        // The final line should appear in the snapshot at row 0 — it's
        // the only content `clear` left behind.
        let mut g = BlockGrid::new(40, 5);
        g.feed(b"first\r\nsecond\r\nthird\r\n");
        // Matches macOS `clear` (terminfo `xterm-256color`):
        //   ESC [ H        cursor home
        //   ESC [ 2 J      erase entire display
        //   ESC [ 3 J      erase scrollback (xterm extension)
        g.feed(b"\x1b[H\x1b[2J\x1b[3J");
        g.feed(b"After clear\r\n");
        let snap = g.snapshot(&palette());
        let mut row0 = String::new();
        for col in 0..snap.cols as usize {
            let ch = snap.cells[col].ch;
            if ch == 0 {
                row0.push(' ');
            } else if let Some(c) = char::from_u32(ch) {
                row0.push(c);
            }
        }
        assert!(
            row0.trim_start().starts_with("After clear"),
            "expected 'After clear' on row 0, got {row0:?}"
        );
    }

    #[test]
    fn rightmost_column_is_rendered() {
        // The user reported a missing character in a long wrapped
        // command echo. This test feeds N copies of "1234567890" into
        // a 46-col grid and asserts every digit lands in some cell —
        // no drops at the wrap boundary.
        let mut g = BlockGrid::new(46, 5);
        let typed = "1234567890".repeat(10); // 100 chars, no \r/\n
        let prefix = "zsh: command not found: ";
        g.feed(prefix.as_bytes());
        g.feed(typed.as_bytes());
        let snap = g.snapshot(&palette());
        let mut digits = String::new();
        for r in 0..snap.rows as usize {
            for c in 0..snap.cols as usize {
                let ch = snap.cells[r * snap.cols as usize + c].ch;
                if let Some(ch) = char::from_u32(ch) {
                    if ch.is_ascii_digit() {
                        digits.push(ch);
                    }
                }
            }
        }
        assert_eq!(
            digits.len(),
            typed.len(),
            "expected {} digits, got {}: {digits:?}",
            typed.len(),
            digits.len()
        );
        assert_eq!(digits, typed, "digit sequence drifted: got {digits:?}");
    }

    #[test]
    fn empty_feed_is_noop() {
        let mut g = BlockGrid::new(10, 3);
        g.feed(&[]);
        let snap = g.snapshot(&palette());
        assert_eq!(snap.cells[0].ch, 0); // blank
    }
}
