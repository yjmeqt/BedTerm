import BedTermCoreC
import Foundation

/// Read-only mirror of a Rust-owned `Block`. Copied out per call —
/// the FFI string pointers are scratch-backed and must not be retained.
public struct RustBlock: Identifiable, Sendable, Equatable {
    public let id: UInt64
    public let command: String
    public let startLine: Int32
    public let endLine: Int32
    public let exitCode: Int32?
    public let duration: TimeInterval?
    public let workingDirectory: String?
    public let gitBranch: String?
    public let cliAgent: CLIAgent?
    public let isRunning: Bool
    public let hasFrozenSnapshot: Bool
}

/// Swift facade over the Rust terminal core.
///
/// Thread safety: not thread-safe. Construct, feed, resize, and snapshot on a
/// single queue (the SSH consume task in BedTermKit is `@MainActor`).
public final class TerminalCore {
    /// OpaquePointer wraps the C `BtTerm *` — the struct body is intentionally
    /// hidden by the Rust-generated header (opaque / forward-declared only).
    private let handle: OpaquePointer
    /// Live PTY screen geometry — kept in sync at `init` / `resize` so
    /// callers (e.g. running-block body sizing) don't need a Rust FFI
    /// round-trip just to read the row count.
    public private(set) var screenCols: Int
    public private(set) var screenRows: Int

    public init(cols: Int, rows: Int) {
        let colsClamped = UInt16(max(1, min(cols, Int(UInt16.max))))
        let rowsClamped = UInt16(max(1, min(rows, Int(UInt16.max))))
        guard let ptr = bt_term_new(colsClamped, rowsClamped) else {
            preconditionFailure("bt_term_new returned NULL")
        }
        self.handle = ptr
        self.screenCols = Int(colsClamped)
        self.screenRows = Int(rowsClamped)
    }

    deinit {
        bt_term_free(handle)
    }

    /// Internal handle for renderer-side FFI. Not for general use.
    var unsafeHandle: OpaquePointer { handle }

    public func feed(_ data: Data) {
        guard !data.isEmpty else { return }
        data.withUnsafeBytes { raw in
            guard let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return }
            bt_term_feed(handle, base, UInt(raw.count))
        }
    }

    public func resize(cols: Int, rows: Int) {
        let colsClamped = UInt16(max(1, min(cols, Int(UInt16.max))))
        let rowsClamped = UInt16(max(1, min(rows, Int(UInt16.max))))
        bt_term_resize(handle, colsClamped, rowsClamped)
        self.screenCols = Int(colsClamped)
        self.screenRows = Int(rowsClamped)
    }

    /// Push a palette (16 ANSI entries + 2 defaults) to the Rust core. Subsequent snapshots resolve
    /// named/indexed/default colours through these values.
    public func setPalette(_ palette: TerminalPalette) {
        precondition(palette.ansi.count == 16, "TerminalPalette.ansi must have exactly 16 entries")
        let ansi = palette.ansi
        // C interop: BtPaletteView.ansi is a Swift tuple (cbindgen surfaces fixed
        // C arrays as tuples), not a Swift Array — list all 16 elements explicitly.
        var view = BtPaletteView(
            default_fg: Self.btRgb(palette.defaultFg),
            default_bg: Self.btRgb(palette.defaultBg),
            ansi: (
                Self.btRgb(ansi[0]), Self.btRgb(ansi[1]),
                Self.btRgb(ansi[2]), Self.btRgb(ansi[3]),
                Self.btRgb(ansi[4]), Self.btRgb(ansi[5]),
                Self.btRgb(ansi[6]), Self.btRgb(ansi[7]),
                Self.btRgb(ansi[8]), Self.btRgb(ansi[9]),
                Self.btRgb(ansi[10]), Self.btRgb(ansi[11]),
                Self.btRgb(ansi[12]), Self.btRgb(ansi[13]),
                Self.btRgb(ansi[14]), Self.btRgb(ansi[15])
            )
        )
        withUnsafePointer(to: &view) { ptr in
            bt_term_set_palette(handle, ptr)
        }
    }

    private static func btRgb(_ component: TerminalPalette.Component) -> BtRgb24 {
        BtRgb24(r: component.r, g: component.g, b: component.b)
    }

    public func snapshot() -> GridSnapshot {
        var view = BtSnapshotView(
            cols: 0, rows: 0, cursor_col: 0, cursor_row: 0,
            display_offset: 0, cells: nil, cell_count: 0)
        guard bt_term_snapshot(handle, &view) == 0, let cellsPtr = view.cells else {
            return GridSnapshot(cols: 0, rows: 0, cursorCol: 0, cursorRow: 0, cells: [])
        }
        let buffer = UnsafeBufferPointer(start: cellsPtr, count: Int(view.cell_count))
        var cells: [GridSnapshot.Cell] = []
        cells.reserveCapacity(Int(view.cell_count))
        for raw in buffer {
            cells.append(GridSnapshot.Cell(ch: raw.ch, fgRGBA: raw.fg_rgba, bgRGBA: raw.bg_rgba, flags: raw.flags))
        }
        bt_term_snapshot_release(handle)
        return GridSnapshot(
            cols: view.cols, rows: view.rows,
            cursorCol: view.cursor_col, cursorRow: view.cursor_row,
            displayOffset: view.display_offset, cells: cells)
    }

    /// Scroll the display by `delta` rows. Positive = into history (up),
    /// negative = toward live bottom (down). Clamped internally to
    /// `[0, scrollbackLines]`.
    public func scrollBy(_ delta: Int) {
        let clamped = Int32(max(min(delta, Int(Int32.max)), Int(Int32.min)))
        bt_term_scroll_by(handle, clamped)
    }

    public func scrollToBottom() {
        bt_term_scroll_to_bottom(handle)
    }

    public var scrollOffset: Int {
        Int(bt_term_scroll_offset(handle))
    }

    public var scrollbackLines: Int {
        Int(bt_term_scrollback_lines(handle))
    }

    /// Current terminal mode flags. Cheap to read (one pointer deref in Rust);
    /// safe to poll after every `feed(_:)`.
    public var mode: BedTermMode {
        BedTermMode(rawValue: bt_term_mode(handle))
    }

    /// Cursor's current grid line on the active screen (0..rows-1). Block
    /// view records this on each block boundary to anchor block ranges.
    public var currentLine: Int32 {
        bt_term_current_line(handle)
    }

    /// Grid-absolute line index of the screen's bottom row. Use this
    /// as the upper bound for a running block's body so cells drawn
    /// below the cursor (TUI redraws via cursor-positioning escapes)
    /// stay visible.
    public var screenBottomLine: Int32 {
        bt_term_screen_bottom_line(handle)
    }

    /// Snapshot a row range from the active screen + scrollback. `start`
    /// inclusive, `end` exclusive; coordinates are grid lines (0 = top of
    /// active screen, negative = into scrollback). Returns `nil` if the
    /// range is empty after clamping. Used by Block view to capture a
    /// sealed block's body at command-end and to re-render a running
    /// block's body every frame.
    public func snapshotRange(startLine: Int32, endLine: Int32) -> GridSnapshot? {
        var view = BtSnapshotView(
            cols: 0, rows: 0, cursor_col: 0, cursor_row: 0,
            display_offset: 0, cells: nil, cell_count: 0)
        guard bt_term_snapshot_range(handle, startLine, endLine, &view) == 0,
            view.rows > 0,
            let cellsPtr = view.cells
        else {
            return nil
        }
        let buffer = UnsafeBufferPointer(start: cellsPtr, count: Int(view.cell_count))
        var cells: [GridSnapshot.Cell] = []
        cells.reserveCapacity(Int(view.cell_count))
        for raw in buffer {
            cells.append(
                GridSnapshot.Cell(
                    ch: raw.ch, fgRGBA: raw.fg_rgba, bgRGBA: raw.bg_rgba, flags: raw.flags))
        }
        bt_term_snapshot_release(handle)
        return GridSnapshot(
            cols: view.cols, rows: view.rows,
            cursorCol: view.cursor_col, cursorRow: view.cursor_row,
            displayOffset: view.display_offset, cells: cells)
    }

    public var blockCount: Int {
        Int(bt_term_block_count(handle))
    }

    public func block(at index: Int) -> RustBlock? {
        // cbindgen surfaces C arrays as Swift tuples; list every field explicitly.
        var view = BtBlockView(
            id: 0,
            start_line: 0,
            end_line: 0,
            is_running: 0,
            has_exit_code: 0,
            _pad: (0, 0),
            exit_code: 0,
            duration_ms: 0,
            has_duration: 0,
            _pad2: (0, 0, 0, 0, 0, 0, 0),
            command: nil,
            command_len: 0,
            cwd: nil,
            cwd_len: 0,
            git_branch: nil,
            git_branch_len: 0,
            has_frozen_snapshot: 0,
            cli_agent: 0,
            _pad3: (0, 0, 0, 0, 0, 0)
        )
        guard index >= 0,
            index < blockCount,
            bt_term_block_at(handle, UInt(index), &view) == 0
        else {
            return nil
        }
        let command = copyOutString(ptr: view.command, len: Int(view.command_len))
        let cwd =
            view.cwd_len > 0
            ? copyOutString(ptr: view.cwd, len: Int(view.cwd_len))
            : nil
        let gitBranch =
            view.git_branch_len > 0
            ? copyOutString(ptr: view.git_branch, len: Int(view.git_branch_len))
            : nil
        return RustBlock(
            id: view.id,
            command: command,
            startLine: view.start_line,
            endLine: view.end_line,
            exitCode: view.has_exit_code != 0 ? view.exit_code : nil,
            duration: view.has_duration != 0
                ? TimeInterval(view.duration_ms) / 1000.0
                : nil,
            workingDirectory: cwd,
            gitBranch: gitBranch,
            cliAgent: CLIAgent(ffiTag: view.cli_agent),
            isRunning: view.is_running != 0,
            hasFrozenSnapshot: view.has_frozen_snapshot != 0
        )
    }

    public func allBlocks() -> [RustBlock] {
        let count = blockCount
        var out: [RustBlock] = []
        out.reserveCapacity(count)
        for idx in 0..<count {
            if let block = block(at: idx) {
                out.append(block)
            }
        }
        return out
    }

    public func frozenSnapshot(forBlockAt index: Int) -> GridSnapshot? {
        var view = BtSnapshotView(
            cols: 0, rows: 0, cursor_col: 0, cursor_row: 0,
            display_offset: 0, cells: nil, cell_count: 0)
        guard bt_term_block_snapshot(handle, UInt(index), &view) == 0,
            view.rows > 0,
            let cellsPtr = view.cells
        else {
            return nil
        }
        let buffer = UnsafeBufferPointer(start: cellsPtr, count: Int(view.cell_count))
        var cells: [GridSnapshot.Cell] = []
        cells.reserveCapacity(Int(view.cell_count))
        for raw in buffer {
            cells.append(
                GridSnapshot.Cell(
                    ch: raw.ch, fgRGBA: raw.fg_rgba, bgRGBA: raw.bg_rgba, flags: raw.flags))
        }
        bt_term_block_snapshot_release(handle)
        return GridSnapshot(
            cols: view.cols, rows: view.rows,
            cursorCol: view.cursor_col, cursorRow: view.cursor_row,
            displayOffset: view.display_offset, cells: cells)
    }

    private func copyOutString(ptr: UnsafePointer<UInt8>?, len: Int) -> String {
        guard let ptr, len > 0 else { return "" }
        let buf = UnsafeBufferPointer(start: ptr, count: len)
        return String(bytes: buf, encoding: .utf8) ?? ""
    }
}
