import BedTermCoreC
import Foundation

/// Swift facade over the Rust terminal core.
///
/// Thread safety: not thread-safe. Construct, feed, resize, and snapshot on a
/// single queue (the SSH consume task in BedTermKit is `@MainActor`).
public final class TerminalCore {
    /// OpaquePointer wraps the C `BtTerm *` — the struct body is intentionally
    /// hidden by the Rust-generated header (opaque / forward-declared only).
    private let handle: OpaquePointer

    public init(cols: Int, rows: Int) {
        let colsClamped = UInt16(max(1, min(cols, Int(UInt16.max))))
        let rowsClamped = UInt16(max(1, min(rows, Int(UInt16.max))))
        guard let ptr = bt_term_new(colsClamped, rowsClamped) else {
            preconditionFailure("bt_term_new returned NULL")
        }
        self.handle = ptr
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
    }

    public func snapshot() -> GridSnapshot {
        var view = BtSnapshotView(cols: 0, rows: 0, cursor_col: 0, cursor_row: 0, cells: nil, cell_count: 0)
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
        return GridSnapshot(cols: view.cols, rows: view.rows, cursorCol: view.cursor_col, cursorRow: view.cursor_row, cells: cells)
    }
}
