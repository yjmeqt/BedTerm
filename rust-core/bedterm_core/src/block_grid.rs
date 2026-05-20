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
        let rows = self.rows;
        let mut cells = Vec::with_capacity(cols as usize * rows as usize);
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
        let cursor_row = if (0..rows as i32).contains(&cursor.line.0) {
            cursor.line.0 as u16
        } else {
            rows
        };
        GridSnapshot {
            cols,
            rows,
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
        assert_eq!(snap.rows, 5);
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
    fn empty_feed_is_noop() {
        let mut g = BlockGrid::new(10, 3);
        g.feed(&[]);
        let snap = g.snapshot(&palette());
        assert_eq!(snap.cells[0].ch, 0); // blank
    }
}
