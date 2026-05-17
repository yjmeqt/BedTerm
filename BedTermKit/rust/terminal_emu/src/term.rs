//! `TermEmu` wraps alacritty_terminal for use across FFI.
//!
//! It maintains a flat `Vec<Cell>` cache so `grid_snapshot()` can return a
//! stable `*const Cell` pointer that Swift can read without serialisation.

use crate::color::AnsiPalette;
use crate::grid::{Cell, DirtyRange, TerminalGrid};
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::cell::Flags as AFlags;
use alacritty_terminal::term::color::Color as AColor;
use alacritty_terminal::term::SizeInfo;
use alacritty_terminal::term::Term;

/// No-op event listener — we poll the grid rather than reacting to events.
struct EventProxy;

impl EventListener for EventProxy {
    fn send_event(&self, _event: Event) {}
}

/// A terminal emulator wrapping alacritty_terminal.
pub struct TermEmu {
    inner: Term<EventProxy>,
    /// Flattened cell cache for FFI snapshot.
    cells: Vec<Cell>,
    /// Per-line dirty flags (index = absolute grid row).
    line_dirty: Vec<bool>,
    /// Dirty ranges computed after each feed.
    dirty_ranges: Vec<DirtyRange>,
    /// Current palette.
    palette: AnsiPalette,
    /// Viewport dimensions.
    cols: usize,
    rows: usize,
}

impl TermEmu {
    pub fn new(cols: usize, rows: usize, scrollback_limit: Option<usize>) -> Self {
        let config = alacritty_terminal::term::Config::default();
        let size = SizeInfo {
            width: cols as f32 * 8.0,
            height: rows as f32 * 16.0,
            cell_width: 8.0,
            cell_height: 16.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };

        let mut term = Term::new(config, &size, EventProxy);
        if let Some(limit) = scrollback_limit {
            term.set_scrollback_limit(limit);
        }

        let mut this = Self {
            inner: term,
            cells: Vec::with_capacity(cols * rows),
            line_dirty: Vec::new(),
            dirty_ranges: Vec::new(),
            palette: AnsiPalette::dark(),
            cols,
            rows,
        };
        this.rebuild_cell_cache();
        this.mark_all_dirty();
        this
    }

    /// Feed bytes into the terminal parser.
    /// Returns the number of dirty ranges produced.
    pub fn feed(&mut self, bytes: &[u8]) -> u32 {
        self.dirty_ranges.clear();
        self.line_dirty.fill(false);

        // Record scroll offset before parsing so we can detect scroll events.
        let pre_display_offset = self.inner.grid().display_offset();

        self.inner.advance_parser(bytes);

        // Detect dirty rows.
        // After advance_parser, cells in the visible area may have changed.
        // We mark the full visible range as dirty for simplicity in Phase 1;
        // a future optimisation can diff individual rows.
        let post_display_offset = self.inner.grid().display_offset();
        let screen_lines = self.inner.screen_lines() as usize;

        // Mark visible rows dirty if anything changed.
        let changed = pre_display_offset != post_display_offset || !bytes.is_empty();
        if changed {
            // Mark the visible viewport dirty.
            let abs_start = post_display_offset;
            let abs_end = abs_start + screen_lines;

            self.dirty_ranges.push(DirtyRange {
                start_row: abs_start as u32,
                end_row: (abs_end as u32).min(self.line_dirty.len() as u32),
            });
        }

        // Rebuild the flat cell cache for the snapshot.
        self.rebuild_cell_cache();

        self.dirty_ranges.len() as u32
    }

    /// Return a snapshot of the current grid. Pointers are stable until the
    /// next mutable call on `self`.
    pub fn grid_snapshot(&self) -> TerminalGrid {
        let grid = self.inner.grid();
        let display_offset = grid.display_offset();
        let screen_lines = self.inner.screen_lines() as usize;
        let cursor = grid.cursor();

        TerminalGrid {
            cells: self.cells.as_ptr(),
            cols: self.cols as u16,
            rows: self.rows as u16,
            total_scrollback: self.inner.total_lines() as u32,
            viewport_top: display_offset as u32,
            cursor_col: cursor.point.column.0 as u16,
            cursor_row: (cursor.point.line.0 as i32 - display_offset as i32).max(0) as u16,
            cursor_visible: {
                // Cursor is visible when it's within the viewport and not hidden.
                let cursor_line = cursor.point.line.0;
                cursor_line >= display_offset
                    && cursor_line < display_offset + screen_lines
            },
            cursor_style: match cursor.shape {
                alacritty_terminal::term::cursor::CursorShape::Block => 0,
                alacritty_terminal::term::cursor::CursorShape::Underline => 1,
                alacritty_terminal::term::cursor::CursorShape::Beam => 2,
                _ => 0,
            },
            dirty: self.dirty_ranges.as_ptr(),
            dirty_count: self.dirty_ranges.len() as u32,
        }
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        let size = SizeInfo {
            width: cols as f32 * 8.0,
            height: rows as f32 * 16.0,
            cell_width: 8.0,
            cell_height: 16.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        self.inner.resize(size);
        self.rebuild_cell_cache();
        self.mark_all_dirty();
    }

    /// Get the selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        // alacritty_terminal selection is accessed through the grid.
        // For Phase 1, return None — selection handling will be added
        // once the rendering pipeline is stable.
        None
    }
}

// ── Private helpers ─────────────────────────────────────────────────

impl TermEmu {
    /// Rebuild the flat `cells` vector from the alacritty_terminal grid so
    /// `grid_snapshot()` can hand out a stable pointer.
    fn rebuild_cell_cache(&mut self) {
        let grid = self.inner.grid();
        let display_offset = grid.display_offset();
        let cols = self.cols;
        let total_lines = self.inner.total_lines();
        let cap = cols * total_lines;
        self.cells.clear();
        self.cells.reserve(cap);

        // Iterate all lines (scrollback + visible) in display order.
        for line_idx in 0..total_lines {
            let row = grid.buffer_ref(line_idx);
            match row {
                Some(row) => {
                    for col in 0..cols {
                        let cell = row.get(col);
                        let flags = cell.flags;
                        let ch = cell.c;
                        self.cells.push(Cell {
                            codepoint: ch as u32,
                            fg: Self::map_color(cell.fg, &self.palette),
                            bg: Self::map_color(cell.bg, &self.palette),
                            flags: Self::map_flags(flags),
                            width: if flags.contains(AFlags::WIDE_CHAR) { 2 } else { 1 },
                        });
                    }
                }
                None => {
                    // Empty line (shouldn't normally happen in the grid).
                    for _ in 0..cols {
                        self.cells.push(Cell::empty());
                    }
                }
            }
        }

        self.line_dirty.resize(total_lines, false);
    }

    fn mark_all_dirty(&mut self) {
        let total = self.inner.total_lines();
        self.dirty_ranges.clear();
        self.dirty_ranges.push(DirtyRange {
            start_row: 0,
            end_row: total as u32,
        });
    }

    fn map_color(acolor: AColor, palette: &AnsiPalette) -> crate::grid::Color {
        match acolor {
            AColor::Named(named) => {
                let idx = match named {
                    alacritty_terminal::term::color::NamedColor::Black => 0,
                    alacritty_terminal::term::color::NamedColor::Red => 1,
                    alacritty_terminal::term::color::NamedColor::Green => 2,
                    alacritty_terminal::term::color::NamedColor::Yellow => 3,
                    alacritty_terminal::term::color::NamedColor::Blue => 4,
                    alacritty_terminal::term::color::NamedColor::Magenta => 5,
                    alacritty_terminal::term::color::NamedColor::Cyan => 6,
                    alacritty_terminal::term::color::NamedColor::White => 7,
                    alacritty_terminal::term::color::NamedColor::BrightBlack => 8,
                    alacritty_terminal::term::color::NamedColor::BrightRed => 9,
                    alacritty_terminal::term::color::NamedColor::BrightGreen => 10,
                    alacritty_terminal::term::color::NamedColor::BrightYellow => 11,
                    alacritty_terminal::term::color::NamedColor::BrightBlue => 12,
                    alacritty_terminal::term::color::NamedColor::BrightMagenta => 13,
                    alacritty_terminal::term::color::NamedColor::BrightCyan => 14,
                    alacritty_terminal::term::color::NamedColor::BrightWhite => 15,
                    alacritty_terminal::term::color::NamedColor::BrightForeground => 15,
                    alacritty_terminal::term::color::NamedColor::Foreground => 7,
                    _ => 7,
                };
                let rgb = palette.rgb(idx);
                crate::grid::Color { r: rgb.0, g: rgb.1, b: rgb.2, ansi_index: idx }
            }
            AColor::Indexed(idx) => {
                let rgb = palette.rgb(idx as u8);
                crate::grid::Color { r: rgb.0, g: rgb.1, b: rgb.2, ansi_index: idx as u8 }
            }
            AColor::Spec(r, g, b) => {
                crate::grid::Color { r, g, b, ansi_index: 0xFF }
            }
        }
    }

    fn map_flags(flags: AFlags) -> u16 {
        let mut out: u16 = 0;
        if flags.contains(AFlags::BOLD) { out |= 0b0000_0001; }
        if flags.contains(AFlags::ITALIC) { out |= 0b0000_0010; }
        if flags.contains(AFlags::UNDERLINE) { out |= 0b0000_0100; }
        if flags.contains(AFlags::INVERSE) { out |= 0b0000_1000; }
        if flags.contains(AFlags::DIM) { out |= 0b0001_0000; }
        if flags.contains(AFlags::STRIKEOUT) { out |= 0b0010_0000; }
        out
    }
}
