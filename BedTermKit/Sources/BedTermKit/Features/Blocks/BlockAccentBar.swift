import UIKit

/// Per-block left-edge accent stripe — Warp's iconic visual.
/// `BlockListContainerViewController` mounts/demounts one of these
/// alongside each visible block header (same virtualisation window),
/// so off-screen accent bars don't accumulate `UIView`s.
@MainActor
enum BlockAccentBar {
    static let widthPt: CGFloat = 3

    static func make() -> UIView {
        let bar = UIView()
        bar.layer.cornerRadius = widthPt / 2
        bar.layer.maskedCorners = [.layerMaxXMinYCorner, .layerMaxXMaxYCorner]
        bar.isUserInteractionEnabled = false
        return bar
    }

    static func color(for block: Block) -> UIColor {
        if block.isRunning {
            return uiColor("ShadcnMutedForeground", fallback: .systemGray)
        }
        switch block.exitCode {
        case .some(0):
            return uiColor("ShadcnPrimary", fallback: .systemBlue)
        case .some:
            return uiColor("ShadcnDestructive", fallback: .systemRed)
        case .none:
            return uiColor("ShadcnMutedForeground", fallback: .systemGray)
        }
    }

    /// Animate the bar (running) or hold it solid (sealed). Pulse is
    /// driven by Core Animation, independent of the parent's
    /// CADisplayLink — no per-frame Swift work.
    static func applyPulse(to bar: UIView, running: Bool) {
        if running {
            if bar.layer.animation(forKey: "pulse") != nil { return }
            let anim = CABasicAnimation(keyPath: "opacity")
            anim.fromValue = 1.0
            anim.toValue = 0.35
            anim.duration = 0.9
            anim.autoreverses = true
            anim.repeatCount = .infinity
            anim.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            bar.layer.add(anim, forKey: "pulse")
        } else {
            bar.layer.removeAnimation(forKey: "pulse")
            bar.layer.opacity = 1.0
        }
    }

    private static func uiColor(_ name: String, fallback: UIColor) -> UIColor {
        UIColor(named: name, in: .module, compatibleWith: nil) ?? fallback
    }
}
