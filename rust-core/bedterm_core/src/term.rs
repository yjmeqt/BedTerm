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
pub(crate) const SCROLLBACK_LINES: u16 = 10_000;

/// Minimal Dimensions implementation. `total_lines` includes scrollback so the
/// alacritty grid actually allocates history.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dims {
    pub(crate) cols: u16,
    pub(crate) screen_rows: u16,
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

/// Callback invoked when a block is sealed (CommandFinished). The closure
/// receives a shared reference to the just-finalized block. Mutable so
/// the closure can update its own captured state (e.g. sequence counters).
/// `Send` is required only so `Terminal` can be `Send`; in practice both
/// the `Terminal` and the `Database` it references live on the same thread
/// (Swift MainActor for live sessions, or the test thread for in-memory
/// tests).
type BlockSink = Box<dyn FnMut(&crate::blocks::Block) + Send + 'static>;

pub struct Terminal {
    parser: Processor,
    term: Term<VoidListener>,
    cols: u16,
    rows: u16,
    palette: Palette,
    dcs: crate::dcs::DcsSniffer,
    blocks: crate::blocks::BlockStore,
    block_sink: Option<BlockSink>,
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
            dcs: crate::dcs::DcsSniffer::new(),
            blocks: crate::blocks::BlockStore::new(),
            block_sink: None,
        }
    }

    /// Wire this `Terminal` to a persistence `Database` so that every time a
    /// command block finalizes (CommandFinished) a row is inserted into the
    /// `blocks` table of `db`.
    ///
    /// # Ownership / lifetime contract
    ///
    /// **Raw-pointer pattern.** `Database` holds a `rusqlite::Connection`
    /// which is `!Sync` — it cannot be placed inside an `Arc<Mutex<>>` without
    /// paying unnecessary synchronisation overhead for what is intentionally a
    /// single-threaded data path. Instead we capture a raw pointer to `db` and
    /// document the invariant:
    ///
    /// > The caller (Swift's MainActor or the test thread) MUST ensure that
    /// > `db` lives at least as long as `self`. The typical call site is
    /// > `bedterm_persistence_attach` (FFI), which holds both `handle` (owns
    /// > the `Database`) and `terminal` behind heap-allocated, opaque handles
    /// > that are freed in order: terminal first, then handle.
    ///
    /// An alternative would be `Rc<RefCell<Database>>`, which makes the
    /// single-threaded contract explicit in the type system but forces callers
    /// to use `Rc` everywhere, complicating the FFI boundary with extra
    /// indirection and Box-wrap cost. The raw-pointer approach is simpler and
    /// matches the plan's recommended pattern.
    ///
    /// # `started_at` / `finished_at` timestamps
    ///
    /// `Block` does not store a wall-clock `started_at` timestamp — only
    /// `duration_ms` (computed from `Instant` deltas). Therefore:
    ///   - `finished_at` = `unix_seconds_now()` at the moment of sealing.
    ///   - `started_at`  = `finished_at - duration_ms/1000.0` when
    ///     `duration_ms` is `Some`, or `finished_at` when it is `None`
    ///     (partial-integration path where no matching Preexec arrived).
    pub fn attach_persistence(&mut self, db: &crate::persistence::Database, snapshot_id: &str) {
        let snapshot_id = snapshot_id.to_string();
        // SAFETY: `db_ptr` is valid for the lifetime of the
        // Terminal/Database pairing — the caller guarantees that the
        // `Database` (or its owning `PersistenceHandle`) outlives `self`.
        let db_ptr = db as *const crate::persistence::Database as usize;
        self.block_sink = Some(Box::new(move |block: &crate::blocks::Block| {
            let db = unsafe { &*(db_ptr as *const crate::persistence::Database) };
            let finished_at = crate::persistence::unix_seconds_now();
            let started_at = match block.duration_ms {
                Some(ms) => finished_at - ms as f64 / 1000.0,
                None => finished_at,
            };
            let row = crate::persistence::BlockRow {
                snapshot_id: snapshot_id.clone(),
                block_seq: block.id as i64,
                command: block.command.clone(),
                stylized_command: block.stylized_command.clone(),
                stylized_output: block.stylized_output.clone(),
                exit_code: block.exit_code,
                cwd: block.working_directory.clone(),
                git_branch: block.git_branch.clone(),
                started_at,
                finished_at,
            };
            let _ = db.insert_block(&row);
        }));
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        // Per-block routing: bytes between `Preexec` and the matching
        // `CommandFinished` go to the running block's private grid;
        // every other byte (prompt drawing, between-command output)
        // goes to the global terminal. Mirrors Warp's per-block grid
        // model.
        //
        // We rely on the existing stateful `DcsSniffer` (alacritty's
        // VTE parser) to emit events because DCS frames routinely
        // span multiple `feed` calls in practice — we observed shell
        // integration ship a single frame as three separate writes
        // (`\eP$d` start, hex body, `\e\` terminator). A stateless
        // scanner over one chunk would miss every cross-chunk frame.
        //
        // `feed_with_positions` returns each newly-completed event
        // paired with its end-of-frame byte offset in `bytes`. Walk
        // the input as (chunk-up-to-and-including-the-frame, event)
        // pairs:
        //   1. Feed bytes through `cursor..event.end` to the current
        //      target (DCS frame included — alacritty silently
        //      swallows `$d`-final DCS so this is a no-op for the
        //      grid but keeps its parser state happy).
        //   2. Apply the event — may attach a new block grid
        //      (`Preexec`) or seal the current one (`CommandFinished`).
        //   3. Continue past the frame.
        let cols = self.cols;
        let rows = self.rows;
        let now = std::time::Instant::now();
        let events = self.dcs.feed_with_positions(bytes);

        let mut cursor = 0usize;
        for (frame_start, end_pos, event) in &events {
            // Dispatch the full chunk including the DCS frame to the VTE
            // grid parser (alacritty silently swallows `$d`-final DCS).
            self.dispatch_chunk(&bytes[cursor..*end_pos]);
            // Capture raw content bytes — but NOT the DCS frame itself.
            // `frame_start` is the offset of the ESC introducer within
            // this call's `bytes`. Bytes before the frame are genuine
            // output content; the frame is protocol signalling and must
            // be excluded so the stylized buffer doesn't accumulate DCS
            // overhead on every Precmd/CommandFinished boundary.
            let content_end = frame_start.unwrap_or(*end_pos);
            if content_end > cursor {
                self.capture_output_bytes(&bytes[cursor..content_end]);
            }
            let current = self.current_grid_absolute_line();
            // Detect whether this event will cause a block to be sealed
            // BEFORE calling apply — `has_open_block` reflects the pre-apply
            // state. `CommandFinished` always seals the open block (if any);
            // `Precmd` seals an open block on the Ctrl-C path (i.e. when
            // there IS an open block at the time Precmd fires).
            let will_seal = match event {
                crate::dcs::DcsEvent::CommandFinished { .. } => self.blocks.has_open_block(),
                crate::dcs::DcsEvent::Precmd { .. } => self.blocks.has_open_block(),
                crate::dcs::DcsEvent::Preexec { .. } => false,
            };
            self.blocks
                .apply(event, current, now, cols, rows, &self.palette);
            // After apply, fire the persistence sink if a block was just
            // sealed. The sink receives a shared reference to the sealed
            // block, which is now fully populated (exit_code, stylized_output,
            // frozen_snapshot all set by `seal_with`). We take-and-restore
            // the sink to avoid a simultaneous mutable borrow of `self`.
            if will_seal && self.block_sink.is_some() {
                if let Some(block) = self.blocks.last_finalized() {
                    // Safety valve against calling the sink with a stale
                    // (previously sealed) block: `last_finalized` returns
                    // the most-recently-sealed block. Since `will_seal` was
                    // true and the last `apply` sealed exactly one block,
                    // that is the correct block.
                    if let Some(mut sink) = self.block_sink.take() {
                        sink(block);
                        self.block_sink = Some(sink);
                    }
                }
            }
            cursor = *end_pos;
        }
        // Trailing bytes after the last event (or the entire chunk
        // if no event completed this call — most chunks).
        self.dispatch_chunk(&bytes[cursor..]);
        if cursor < bytes.len() {
            self.capture_output_bytes(&bytes[cursor..]);
        }
    }

    /// Feed `bytes` to the currently-active target — the open block's
    /// private grid if a command is running, else the global terminal.
    fn dispatch_chunk(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut also_global = true;
        if let Some(block) = self.blocks.open_block_mut() {
            if let Some(grid) = block.grid.as_mut() {
                // When a TUI inside the running block is in alt-screen
                // (vim / htop / claude / less / fzf) the host UI flips
                // to the classic full-screen renderer which paints from
                // the *global* term. We need the global term to see the
                // same alt-screen bytes so it can render the TUI and,
                // crucially, the `\e[?1049l` exit escape — without that
                // the global term gets stuck in alt-screen even after
                // the TUI quits.
                //
                // Sample alt-screen state on both sides of the grid
                // feed and union them. Without the post-feed sample we
                // would miss the entry escape (state was off before
                // feeding, on after) and global would never enter alt-
                // screen at all. Without the pre-feed sample we would
                // miss the exit escape (state was on before, off after)
                // and global would never leave.
                let was_alt = grid.term().mode().contains(TermMode::ALT_SCREEN);
                grid.feed(bytes);
                let is_alt = grid.term().mode().contains(TermMode::ALT_SCREEN);
                also_global = was_alt || is_alt;
            }
        }
        if also_global {
            self.parser.advance(&mut self.term, bytes);
        }
    }

    /// Append `bytes` to the open block's `stylized_output` buffer, if a
    /// block is currently running (i.e. `Preexec` has fired and
    /// `CommandFinished` has not). DCS frame bytes must be excluded by the
    /// caller — this method appends whatever it receives verbatim.
    fn capture_output_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Some(block) = self.blocks.open_block_mut() {
            if block.grid.is_some() {
                block.append_output_bytes(bytes);
            }
        }
    }

    /// Grid-absolute current line, sampled from the *active* grid —
    /// the open block's private grid when one is attached (cursor
    /// has no history offset there), else the global terminal's
    /// cursor + scrollback offset.
    fn current_grid_absolute_line(&self) -> i32 {
        if let Some(block) = self.blocks.blocks().iter().rev().find(|b| b.is_running) {
            if let Some(grid) = block.grid.as_ref() {
                return grid.cursor_row();
            }
        }
        let grid = self.term.grid();
        grid.history_size() as i32 + grid.cursor.point.line.0
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let dims = Dims {
            cols,
            screen_rows: rows,
        };
        self.term.resize(dims);
        self.blocks.resize_grids(cols, rows);
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
        // Union of the global term's mode and any running block's
        // private-grid mode. With per-block VTE routing, full-screen
        // TUIs (vim/htop/claude) send their alt-screen + mouse-mode
        // escapes into the running block's `BlockGrid`, never touching
        // the global term. Without unioning here the host would never
        // see `ALT_SCREEN` and would keep showing the block-list +
        // composer instead of switching to the live grid view, leaving
        // the TUI unreachable.
        let mut m = *self.term.mode();
        if let Some(block) = self.blocks.blocks().iter().rev().find(|b| b.is_running) {
            if let Some(grid) = block.grid.as_ref() {
                m |= *grid.term().mode();
            }
        }
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

    /// Borrow the current palette. Used by the renderer when freezing
    /// a per-block live grid into the cell stream — every cell needs
    /// the same colour resolution as the global grid.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// Grid-absolute line index of the screen's bottom row, regardless of
    /// where the cursor sits. Running TUIs (claude, fzf) draw cells with
    /// cursor-positioning escapes that land *below* the current cursor
    /// row, so the cursor-based row count under-reports a running block's
    /// extent. Use this as the upper bound for a running block's body.
    pub fn screen_bottom_line(&self) -> i32 {
        let grid = self.term.grid();
        grid.history_size() as i32 + self.rows as i32 - 1
    }

    /// Grid-absolute line of the cursor (`history_size() + screen_line`).
    /// Monotonically non-decreasing as output scrolls — every newline that
    /// pushes content into scrollback bumps `history_size`. Block start /
    /// end lines are stored in this same absolute frame; the row count
    /// `end - start` therefore equals the rows the command actually
    /// consumed. `snapshot_range` translates back to alacritty's
    /// screen-relative space internally.
    pub fn current_line(&self) -> i32 {
        let grid = self.term.grid();
        grid.history_size() as i32 + grid.cursor.point.line.0
    }

    /// Snapshot a row range from the active screen + scrollback. Coordinates
    /// are grid lines: `0` is the top of the active screen, negatives index
    /// into history. `start_line` inclusive, `end_line` exclusive. Lines
    /// outside the grid's current extent are clamped; if the entire range
    /// has fallen off scrollback the returned snapshot has `rows == 0`.
    pub fn snapshot_range(&self, start_line: i32, end_line: i32) -> GridSnapshot {
        Self::snapshot_range_from(
            &self.term,
            self.cols,
            self.rows,
            &self.palette,
            start_line,
            end_line,
        )
    }

    /// Implementation of `snapshot_range` that takes its dependencies as
    /// explicit parameters instead of through `&self`. Lets `feed` build a
    /// snapshot closure for `BlockStore::apply` without conflicting with the
    /// `&mut self.blocks` borrow.
    fn snapshot_range_from(
        term: &Term<VoidListener>,
        cols: u16,
        rows: u16,
        palette: &Palette,
        start_line: i32,
        end_line: i32,
    ) -> GridSnapshot {
        let grid = term.grid();
        let history = grid.history_size() as i32;
        // Callers pass grid-absolute lines (see `current_line` docs).
        // alacritty's `grid[Line(l)]` expects screen-relative l in
        // `[-history, rows-1]`, so subtract `history` to translate.
        // Absolute lines that have fallen off scrollback land below
        // `-history` after subtraction and get clamped out.
        let rel_start = start_line - history;
        let rel_end = end_line - history;
        let min_line = -history;
        let max_line = rows as i32;
        let start = rel_start.max(min_line).min(max_line);
        let end = rel_end.max(start).min(max_line);
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
                    fg_rgba: color_to_rgba(cell.fg, palette),
                    bg_rgba: color_to_rgba(cell.bg, palette),
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

    /// Read-only view of the per-command blocks that the DCS sniffer has
    /// produced so far. Driven automatically by `feed`.
    pub fn blocks(&self) -> &[crate::blocks::Block] {
        self.blocks.blocks()
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn block_at(&self, idx: usize) -> Option<&crate::blocks::Block> {
        self.blocks.get(idx)
    }

    /// Drop all accumulated blocks. Resize does NOT call this — only the
    /// host opts in (e.g. when reconnecting an SSH session).
    pub fn reset_blocks(&mut self) {
        self.blocks.reset();
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

pub(crate) fn color_to_rgba(c: alacritty_terminal::vte::ansi::Color, palette: &Palette) -> u32 {
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

#[cfg(test)]
mod block_integration_tests {
    use super::*;

    fn hex(input: &str) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(input.len() * 2);
        for b in input.bytes() {
            write!(out, "{b:02x}").unwrap();
        }
        out
    }

    /// `ESC P $ d <hex(JSON)> 0x9C` — Warp's wire frame.
    fn dcs(json: &str) -> Vec<u8> {
        let mut v = vec![0x1B, b'P', b'$', b'd'];
        v.extend_from_slice(hex(json).as_bytes());
        v.push(0x9C);
        v
    }

    /// `ESC P $ d <hex(JSON)> ESC \` — the 7-bit ST form actually
    /// emitted by `bedterm-integration.sh`. Matters because the VTE
    /// parser handles ESC + `\` differently from the 8-bit 0x9C
    /// terminator, and we hit a wild-`\`-into-block-grid bug with the
    /// 7-bit form on real hardware.
    fn dcs7(json: &str) -> Vec<u8> {
        let mut v = vec![0x1B, b'P', b'$', b'd'];
        v.extend_from_slice(hex(json).as_bytes());
        v.extend_from_slice(b"\x1b\\");
        v
    }

    #[test]
    fn seven_bit_st_does_not_leak_backslash_into_block_grid() {
        // Reproduces a production bug where the first cell of every
        // sealed block was a literal `\` (0x5c). Suspected the trailing
        // `\` of `ESC \` ST leaks past `feed_with_positions`'s reported
        // end_pos when the parser fires `unhook` before consuming the
        // final byte.
        let mut term = Terminal::new(20, 5);
        term.feed(&dcs7(r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#));
        term.feed(&dcs7(r#"{"hook":"Preexec","value":{"command":"ls"}}"#));
        term.feed(b"hello\r\n");
        term.feed(&dcs7(
            r#"{"hook":"CommandFinished","value":{"exit_code":0}}"#,
        ));
        let blocks = term.blocks();
        assert_eq!(blocks.len(), 1);
        let snap = blocks[0].frozen_snapshot.as_ref().expect("snap");
        let text = snapshot_to_string(snap);
        assert!(
            !text.contains('\\'),
            "block grid leaked a literal backslash — got {text:?}"
        );
        assert!(
            text.contains("hello"),
            "block grid missing payload — got {text:?}"
        );
    }

    #[test]
    fn feed_populates_block_store() {
        let mut term = Terminal::new(80, 24);
        term.feed(&dcs(r#"{"hook":"Precmd","value":{"pwd":"/tmp"}}"#));
        term.feed(&dcs(r#"{"hook":"Preexec","value":{"command":"ls"}}"#));
        term.feed(&dcs(
            r#"{"hook":"CommandFinished","value":{"exit_code":0}}"#,
        ));
        let blocks = term.blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].command, "ls");
        assert!(!blocks[0].is_running);
        assert_eq!(blocks[0].exit_code, Some(0));
        // duration is wall-clock — just assert it's Some.
        assert!(blocks[0].duration_ms.is_some());
    }

    #[test]
    fn feed_does_not_strand_running_block_when_only_precmd_arrives() {
        let mut term = Terminal::new(80, 24);
        term.feed(&dcs(r#"{"hook":"Precmd","value":{"pwd":""}}"#));
        let blocks = term.blocks();
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].is_running);
    }

    /// Verifies the per-block-grid byte-routing introduced when we
    /// switched away from the single-grid model. Bytes between
    /// `Preexec` and `CommandFinished` must land in the running
    /// block's private grid (`block.grid`) and *not* in the global
    /// terminal — that's the whole point of the refactor.
    #[test]
    fn feed_routes_command_output_to_block_grid_not_global() {
        let mut term = Terminal::new(20, 5);

        // Build one big chunk: Precmd ▸ "BEFORE" prompt prose ▸
        // Preexec ▸ "INSIDE" command output ▸ CommandFinished ▸
        // "AFTER" next prompt prose.
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&dcs(r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#));
        chunk.extend_from_slice(b"BEFORE");
        chunk.extend_from_slice(&dcs(r#"{"hook":"Preexec","value":{"command":"echo"}}"#));
        chunk.extend_from_slice(b"INSIDE");
        chunk.extend_from_slice(&dcs(
            r#"{"hook":"CommandFinished","value":{"exit_code":0}}"#,
        ));
        chunk.extend_from_slice(b"AFTER");
        term.feed(&chunk);

        // The block sealed at CommandFinished; its frozen_snapshot
        // is the snapshot of its block grid. Stringify the row
        // contents and look for our markers.
        let blocks = term.blocks();
        assert_eq!(blocks.len(), 1);
        let snap = blocks[0]
            .frozen_snapshot
            .as_ref()
            .expect("block grid should have produced a snapshot");
        let block_text = snapshot_to_string(snap);
        assert!(
            block_text.contains("INSIDE"),
            "block grid missing the in-command bytes — got {block_text:?}"
        );
        assert!(
            !block_text.contains("BEFORE"),
            "block grid incorrectly received pre-Preexec bytes — got {block_text:?}"
        );
        assert!(
            !block_text.contains("AFTER"),
            "block grid incorrectly received post-CommandFinished bytes — got {block_text:?}"
        );

        // The global terminal should have BEFORE + AFTER but never
        // INSIDE (that went to the block grid).
        let global = term.snapshot();
        let global_text = snapshot_to_string(&global);
        assert!(
            global_text.contains("BEFORE"),
            "global term missing pre-Preexec bytes — got {global_text:?}"
        );
        assert!(
            global_text.contains("AFTER"),
            "global term missing post-CommandFinished bytes — got {global_text:?}"
        );
        assert!(
            !global_text.contains("INSIDE"),
            "global term leaked command-output bytes — got {global_text:?}"
        );
    }

    /// Real-world wire pattern: shell-integration ships a single DCS
    /// frame as **three separate writes** (`\eP$d` prefix, hex body,
    /// `\e\` terminator). A stateless scanner over one chunk misses
    /// the event entirely — caught in production, fixed by switching
    /// to `DcsSniffer::feed_with_positions` (stateful VTE parser
    /// preserves DCS state between calls).
    #[test]
    fn feed_handles_dcs_frame_split_across_chunks() {
        let mut term = Terminal::new(20, 5);
        // Build a Preexec frame, then break it apart at the same
        // boundaries we observed on the wire:
        //   chunk 1: `\eP$d`
        //   chunk 2: hex of body
        //   chunk 3: `\e\`
        let json = r#"{"hook":"Preexec","value":{"command":"hi"}}"#;
        let frame = dcs(json);
        let split1 = 4; // `\eP$d`
        let split2 = frame.len() - 1; // everything up to the last byte (ST is 0x9c, single byte; split it before/after)

        // First open a block via Precmd in one chunk so the Preexec
        // has somewhere to land.
        term.feed(&dcs(r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#));
        // Now stream the Preexec frame piecemeal.
        term.feed(&frame[..split1]);
        term.feed(&frame[split1..split2]);
        term.feed(&frame[split2..]);

        assert_eq!(term.blocks().len(), 1);
        assert_eq!(term.blocks()[0].command, "hi");
        // The Preexec apply also attached a private grid; subsequent
        // output bytes should route there.
        term.feed(b"INSIDE");
        let snap = term.blocks()[0]
            .grid
            .as_ref()
            .expect("grid attached at Preexec")
            .snapshot(term.palette());
        let text: String = snap
            .cells
            .iter()
            .map(|c| {
                if c.ch == 0 {
                    ' '
                } else {
                    char::from_u32(c.ch).unwrap_or('?')
                }
            })
            .collect();
        assert!(
            text.contains("INSIDE"),
            "block grid missed bytes after split-frame Preexec — got {text:?}"
        );
    }

    fn snapshot_to_string(snap: &crate::snapshot::GridSnapshot) -> String {
        snap.cells
            .iter()
            .map(|c| {
                if c.ch == 0 {
                    ' '
                } else {
                    char::from_u32(c.ch).unwrap_or('?')
                }
            })
            .collect()
    }
}
