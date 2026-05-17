import UIKit

final class MetalCursorLayer: CALayer {
    override init() {
        super.init()
        backgroundColor = UIColor.white.withAlphaComponent(0.85).cgColor
        cornerRadius = 1
        startBlink()
    }

    override init(layer: Any) { super.init(layer: layer) }

    required init?(coder: NSCoder) { nil }

    func startBlink() {
        let blink = CABasicAnimation(keyPath: "opacity")
        blink.fromValue = 1.0
        blink.toValue = 0.0
        blink.duration = 0.53
        blink.autoreverses = true
        blink.repeatCount = .infinity
        add(blink, forKey: "blink")
    }

    func update(col: Int, row: Int, cellSize: CGSize) {
        // Disable implicit layout animation so cursor moves discretely.
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        frame = CGRect(
            x: CGFloat(col) * cellSize.width,
            y: CGFloat(row) * cellSize.height,
            width: cellSize.width,
            height: cellSize.height
        )
        CATransaction.commit()
    }
}
