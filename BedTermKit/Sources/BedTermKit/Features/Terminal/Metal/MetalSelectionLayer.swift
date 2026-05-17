import UIKit

struct SelectionRange: Equatable {
    var startRow: Int
    var startCol: Int
    var endRow: Int
    var endCol: Int

    /// Normalised so start <= end in row-major order.
    var normalised: SelectionRange {
        let startsFirst =
            (startRow < endRow)
            || (startRow == endRow && startCol <= endCol)
        if startsFirst { return self }
        return SelectionRange(startRow: endRow, startCol: endCol, endRow: startRow, endCol: startCol)
    }

    /// CGRects per row covered by the selection.
    func rects(cellSize: CGSize, cols: Int) -> [CGRect] {
        let norm = normalised
        guard norm.startRow != norm.endRow || norm.startCol != norm.endCol else { return [] }
        var out: [CGRect] = []
        for row in norm.startRow...norm.endRow {
            let from = (row == norm.startRow) ? norm.startCol : 0
            let to = (row == norm.endRow) ? norm.endCol : cols
            out.append(
                CGRect(
                    x: CGFloat(from) * cellSize.width,
                    y: CGFloat(row) * cellSize.height,
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
        for rect in range.rects(cellSize: cellSize, cols: cols) {
            combined.append(UIBezierPath(rect: rect))
        }
        path = combined.cgPath
    }
}
