import UIKit

final class MetalSelectionGesture: UILongPressGestureRecognizer {
    var onSelectionChange: ((SelectionRange?) -> Void)?
    var onCopy: ((SelectionRange) -> Void)?
    var cellSize: CGSize = .zero

    private var current: SelectionRange?

    override init(target: Any?, action: Selector?) {
        super.init(target: target, action: action)
        minimumPressDuration = 0.4
        addTarget(self, action: #selector(handle(_:)))
    }

    @objc private func handle(_ gr: UIGestureRecognizer) {
        guard let view = gr.view, cellSize.width > 0, cellSize.height > 0 else { return }
        let p = gr.location(in: view)
        let col = max(0, Int(p.x / cellSize.width))
        let row = max(0, Int(p.y / cellSize.height))

        switch gr.state {
        case .began:
            current = SelectionRange(startRow: row, startCol: col, endRow: row, endCol: col + 1)
        case .changed:
            current?.endRow = row
            current?.endCol = col + 1
        case .ended:
            if let sel = current { onCopy?(sel) }
            current = nil
        case .cancelled, .failed:
            current = nil
        default:
            break
        }
        onSelectionChange?(current)
    }
}
