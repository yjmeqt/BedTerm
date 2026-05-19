import UIKit

/// Long-press + drag selection inside a single block's body. Owned by
/// `BlockListContainerViewController`; receives layout metrics + a
/// callback that resolves the block list into the GridSnapshot it
/// needs at hit-test and copy time.
///
/// Selection is gesture-scoped: release copies and clears the
/// highlight. Multi-block selection is out of scope for v1.
@MainActor
final class BlockListSelectionController {
    let longPressGR = UILongPressGestureRecognizer()
    let selectionLayer = MetalSelectionLayer()

    /// Cumulative-Y → block hit. Container computes this from its own
    /// layout walk so this controller doesn't need to duplicate the
    /// header/body math.
    typealias HitResolver = @MainActor (CGPoint) -> BlockHit?

    /// Resolve a block id + current selection range into selected
    /// text. Container owns the BlockStore + TerminalCore needed to
    /// pull the right snapshot.
    typealias TextResolver = @MainActor (UInt64, SelectionRange) -> String?

    struct BlockHit {
        let blockID: UInt64
        let bodyTop: CGFloat
        let rows: Int
        let cols: Int
    }

    private struct ActiveSelection {
        let blockID: UInt64
        let bodyTopInContent: CGFloat
        let cols: Int
        let rows: Int
        var range: SelectionRange
    }

    private weak var contentView: UIView?
    private let hitResolver: HitResolver
    private let textResolver: TextResolver
    private let headerHeightPt: CGFloat
    private var rowHeightPt: CGFloat
    private var cellWidthPt: CGFloat
    private var containerWidth: CGFloat = 0
    /// Horizontal inset between contentView's left edge and the first
    /// cell column — matches the container's Warp-style accent bar.
    private var leftInsetPt: CGFloat = 0
    private var active: ActiveSelection?

    init(
        contentView: UIView,
        headerHeightPt: CGFloat,
        hitResolver: @escaping HitResolver,
        textResolver: @escaping TextResolver
    ) {
        self.contentView = contentView
        self.hitResolver = hitResolver
        self.textResolver = textResolver
        self.headerHeightPt = headerHeightPt
        self.rowHeightPt = 18
        self.cellWidthPt = 9
        contentView.layer.addSublayer(selectionLayer)
        longPressGR.minimumPressDuration = 0.4
        longPressGR.addTarget(self, action: #selector(handleLongPress(_:)))
    }

    /// Container pushes fresh cell metrics each layout pass. Width drives
    /// hit-test column resolution; height drives row resolution + the
    /// selection layer's geometry.
    func updateMetrics(
        cellWidth: CGFloat, rowHeight: CGFloat,
        containerWidth: CGFloat, leftInset: CGFloat
    ) {
        cellWidthPt = cellWidth
        rowHeightPt = rowHeight
        self.containerWidth = containerWidth
        self.leftInsetPt = leftInset
    }

    @objc private func handleLongPress(_ gr: UILongPressGestureRecognizer) {
        guard let contentView else { return }
        let point = gr.location(in: contentView)
        switch gr.state {
        case .began:
            begin(at: point)
        case .changed:
            extend(to: point)
        case .ended:
            finish()
        case .cancelled, .failed:
            cancel()
        default:
            break
        }
    }

    private func begin(at point: CGPoint) {
        guard let hit = hitResolver(point) else {
            cancel()
            return
        }
        let row = clampRow(point.y - hit.bodyTop, rows: hit.rows)
        let col = clampCol(point.x - leftInsetPt, cols: hit.cols)
        active = ActiveSelection(
            blockID: hit.blockID, bodyTopInContent: hit.bodyTop,
            cols: hit.cols, rows: hit.rows,
            range: SelectionRange(
                startRow: row, startCol: col, endRow: row, endCol: col + 1))
        refreshLayer()
    }

    private func extend(to point: CGPoint) {
        guard var sel = active else { return }
        let row = clampRow(point.y - sel.bodyTopInContent, rows: sel.rows)
        let col = clampCol(point.x - leftInsetPt, cols: sel.cols)
        sel.range.endRow = row
        sel.range.endCol = col + 1
        active = sel
        refreshLayer()
    }

    private func finish() {
        if let sel = active, let text = textResolver(sel.blockID, sel.range), !text.isEmpty {
            UIPasteboard.general.string = text
        }
        cancel()
    }

    private func cancel() {
        active = nil
        selectionLayer.update(nil, cellSize: .zero, cols: 0)
    }

    private func refreshLayer() {
        guard let sel = active else {
            selectionLayer.update(nil, cellSize: .zero, cols: 0)
            return
        }
        let bodyHeight = CGFloat(sel.rows) * rowHeightPt
        // Inset the layer so its column-0 rect aligns with the Metal
        // pane's first cell, which is itself inset by the accent bar.
        selectionLayer.frame = CGRect(
            x: leftInsetPt, y: sel.bodyTopInContent,
            width: max(0, containerWidth - leftInsetPt), height: bodyHeight)
        selectionLayer.update(
            sel.range,
            cellSize: CGSize(width: cellWidthPt, height: rowHeightPt),
            cols: sel.cols)
    }

    private func clampRow(_ yPt: CGFloat, rows: Int) -> Int {
        max(0, min(rows - 1, Int(yPt / rowHeightPt)))
    }

    private func clampCol(_ xPt: CGFloat, cols: Int) -> Int {
        max(0, min(cols, Int(xPt / cellWidthPt)))
    }
}
