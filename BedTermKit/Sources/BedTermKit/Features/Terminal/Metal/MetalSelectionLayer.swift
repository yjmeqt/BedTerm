import UIKit

struct SelectionRange: Equatable {
    var startRow: Int
    var startCol: Int
    var endRow: Int
    var endCol: Int

    /// Normalised so start <= end in row-major order.
    var normalised: SelectionRange {
        let startsFirst = (startRow < endRow)
            || (startRow == endRow && startCol <= endCol)
        if startsFirst { return self }
        return SelectionRange(startRow: endRow, startCol: endCol, endRow: startRow, endCol: startCol)
    }

    /// CGRects per row covered by the selection.
    func rects(cellSize: CGSize, cols: Int) -> [CGRect] {
        let n = normalised
        guard n.startRow != n.endRow || n.startCol != n.endCol else { return [] }
        var out: [CGRect] = []
        for r in n.startRow...n.endRow {
            let from = (r == n.startRow) ? n.startCol : 0
            let to   = (r == n.endRow)   ? n.endCol   : cols
            out.append(CGRect(
                x: CGFloat(from) * cellSize.width,
                y: CGFloat(r) * cellSize.height,
                width: CGFloat(to - from) * cellSize.width,
                height: cellSize.height
            ))
        }
        return out
    }
}

final class MetalSelectionLayer: CAShapeLayer {
    override init() {
        super.init()
        fillColor = UIColor.systemBlue.withAlphaComponent(0.35).cgColor
        strokeColor = UIColor.clear.cgColor
    }

    override init(layer: Any) { super.init(layer: layer) }

    required init?(coder: NSCoder) { nil }

    func update(_ range: SelectionRange?, cellSize: CGSize, cols: Int) {
        guard let range else { path = nil; return }
        let combined = UIBezierPath()
        for r in range.rects(cellSize: cellSize, cols: cols) {
            combined.append(UIBezierPath(rect: r))
        }
        path = combined.cgPath
    }
}
