import QuartzCore
import UIKit

/// Explicit scroll-anchor state — mirrors Warp's `ScrollPosition` enum
/// (`block_list_viewport.rs`). Two states cover everything we need
/// today; we can grow this if/when we add anchored scroll inside a
/// long-running block.
///
/// `.followsBottom` — content growth auto-pins the viewport to the
/// bottom (new TUI output, new sealed blocks). Set when the user
/// either lands at the bottom edge of a finished scroll, or when a
/// fresh session starts.
///
/// `.fixedAt(y)` — user actively scrolled away from the bottom.
/// Subsequent content growth must NOT yank the viewport — we just
/// keep the recorded offset. Transition back to `.followsBottom` when
/// the user explicitly scrolls to the bottom edge.
enum ScrollPosition: Equatable {
    case followsBottom
    case fixedAt(CGFloat)
}

/// Scroll-physics constants matching Warp's desktop `MOMENTUM_DECAY`.
private let blockListScrollPhysics = ScrollPhysics(
    decay: 0.968, decayInterval: 0.008)

@MainActor
extension BlockListContainerViewController: UIGestureRecognizerDelegate {
    /// Apply the current `scrollPosition` anchor to `contentOffsetY`.
    /// Skipped during active pan / momentum so the gesture owns the
    /// offset; growing content under a flicking finger can't yank.
    func applyScrollPosition() {
        guard view.bounds.height > 0 else { return }
        guard panGR.state != .began,
            panGR.state != .changed,
            scrollPhysics == nil
        else { return }
        let maxOff = maxOffsetY
        switch scrollPosition {
        case .followsBottom:
            setContentOffsetY(maxOff)
        case .fixedAt(let anchorY):
            let clamped = min(max(0, anchorY), maxOff)
            if abs(contentOffsetY - clamped) > 0.5 {
                setContentOffsetY(clamped)
            }
        }
    }

    /// Single point of mutation for `contentOffsetY`. contentView is
    /// viewport-sized; the selection controller is told to re-anchor
    /// the highlight layer in viewport space so it tracks scroll.
    func setContentOffsetY(_ newOffset: CGFloat) {
        contentOffsetY = newOffset
        selectionController?.notifyScrollOffsetChanged()
    }

    /// Clamp a raw offset to `[0, maxOffsetY]` and report whether the
    /// raw value overflowed.
    func clampOffset(_ offsetY: CGFloat) -> (CGFloat, Bool) {
        let maxOff = maxOffsetY
        let clamped = max(0, min(offsetY, maxOff))
        return (clamped, clamped != offsetY)
    }

    func isAtBottomEdge() -> Bool {
        contentOffsetY >= maxOffsetY - pinTolerancePt
    }

    @objc func handlePan(_ recognizer: UIPanGestureRecognizer) {
        switch recognizer.state {
        case .began:
            stopMomentum()
            panStartOffsetY = contentOffsetY
            scrollPosition = .fixedAt(contentOffsetY)
            velocityEstimator.reset()
            velocityEstimator.push(
                ScrollSample(time: CACurrentMediaTime(), offsetY: contentOffsetY))
        case .changed:
            let translation = recognizer.translation(in: view).y
            let (clamped, _) = clampOffset(panStartOffsetY - translation)
            setContentOffsetY(clamped)
            velocityEstimator.push(
                ScrollSample(time: CACurrentMediaTime(), offsetY: contentOffsetY))
            scrollPosition = isAtBottomEdge() ? .followsBottom : .fixedAt(contentOffsetY)
            pushLayoutToMetalView()
        case .ended, .cancelled:
            let velocity = velocityEstimator.velocity()
            if abs(velocity) > 50 {
                var physics = blockListScrollPhysics
                physics.velocity = velocity
                physics.lastTick = CACurrentMediaTime()
                scrollPhysics = physics
                updateDisplayLink()
            } else if isAtBottomEdge() {
                scrollPosition = .followsBottom
            }
        default:
            break
        }
    }

    func stopMomentum() { scrollPhysics = nil }

    func advanceMomentum(now: CFTimeInterval) {
        guard var physics = scrollPhysics else { return }
        let delta = physics.step(now: now)
        let (clamped, hitEdge) = clampOffset(contentOffsetY + delta)
        setContentOffsetY(clamped)
        if hitEdge || abs(physics.velocity) < 1.0 {
            scrollPhysics = nil
            scrollPosition = isAtBottomEdge() ? .followsBottom : .fixedAt(clamped)
        } else {
            scrollPhysics = physics
        }
    }

    /// Pan + long-press coexistence: when pan starts (movement), it
    /// cancels the in-flight long-press; long-press only fires on a
    /// still finger past `minimumPressDuration`. Returning `false`
    /// gives that single-claim behaviour.
    public func gestureRecognizer(
        _ recognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        false
    }
}

// MARK: - Accessibility scroll

extension BlockListContainerViewController {
    override public func accessibilityScroll(
        _ direction: UIAccessibilityScrollDirection
    ) -> Bool {
        let step = view.bounds.height * 0.8
        switch direction {
        case .down: setContentOffsetY(clampOffset(contentOffsetY + step).0)
        case .up: setContentOffsetY(clampOffset(contentOffsetY - step).0)
        default: return false
        }
        scrollPosition = isAtBottomEdge() ? .followsBottom : .fixedAt(contentOffsetY)
        pushLayoutToMetalView()
        return true
    }
}
