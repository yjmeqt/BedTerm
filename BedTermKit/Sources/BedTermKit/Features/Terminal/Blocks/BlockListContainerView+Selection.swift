import UIKit

/// Selection-resolver helpers split off `BlockListContainerView` so that
/// the controller file stays under SwiftLint's `file_length` cap. The
/// hit-test + text-extraction logic depends only on `session.blockStore`
/// + the layout invariants the main file already exposes.
@MainActor
extension BlockListContainerViewController {
    func blockHitTest(at point: CGPoint) -> BlockListSelectionController.BlockHit? {
        let core = session.terminalCore
        let gap = BlockPanelStyle.interBlockGapPt
        var yPt: CGFloat = 0
        for block in session.blockStore.blocks {
            let bodyTop = yPt + headerHeightPt
            let bodyBot = bodyTop + bodyHeightPt(for: block)
            if point.y >= bodyTop && point.y < bodyBot {
                guard let snap = snapshot(for: block, core: core) else { return nil }
                return .init(
                    blockID: block.id, bodyTop: bodyTop,
                    rows: Int(snap.rows), cols: Int(snap.cols))
            }
            yPt = bodyBot + gap
        }
        return nil
    }

    func snapshot(for block: Block, core: TerminalCore) -> GridSnapshot? {
        if block.hasFrozenSnapshot {
            let frozenIdx = core.allBlocks().firstIndex(where: { $0.id == block.id })
            if let idx = frozenIdx { return core.frozenSnapshot(forBlockAt: idx) }
        }
        if block.isRunning {
            let end = core.currentLine + 1
            if end > block.startLine {
                return core.snapshotRange(startLine: block.startLine, endLine: end)
            }
        }
        return nil
    }

    func extractText(blockID: UInt64, range: SelectionRange) -> String? {
        let core = session.terminalCore
        guard let block = session.blockStore.blocks.first(where: { $0.id == blockID }),
            let snap = snapshot(for: block, core: core)
        else { return nil }
        let norm = range.normalised
        let rowsCount = Int(snap.rows)
        let colsCount = Int(snap.cols)
        let lastRow = min(norm.endRow, rowsCount - 1)
        guard norm.startRow <= lastRow else { return nil }
        var out = ""
        for row in norm.startRow...lastRow {
            let from = (row == norm.startRow) ? norm.startCol : 0
            let to = (row == norm.endRow) ? norm.endCol : colsCount
            for col in from..<min(to, colsCount) {
                let scalar = snap.cell(col: col, row: row).flatMap { cell in
                    cell.ch != 0 ? Unicode.Scalar(cell.ch) : nil
                }
                out.append(scalar.map(Character.init) ?? " ")
            }
            if row != lastRow { out.append("\n") }
        }
        return out
    }
}
