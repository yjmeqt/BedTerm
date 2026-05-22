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

@MainActor
extension BlockListContainerViewController: UIGestureRecognizerDelegate {
    /// Apply the current `scrollPosition` anchor to `contentOffsetY`.
    /// Skipped during active pan / momentum so the gesture owns the
    /// offset; growing content under a flicking finger can't yank.
    func applyScrollPosition() {
        guard view.bounds.height > 0 else { return }
        guard panGR.state != .began,
            panGR.state != .changed,
            momentum == nil
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

    /// Single point of mutation for `contentOffsetY`. Keeps `contentView`
    /// translated in lockstep so the selection layer's content-space
    /// coordinates land at the right pixel.
    func setContentOffsetY(_ newOffset: CGFloat) {
        contentOffsetY = newOffset
        contentView.frame = CGRect(
            x: 0, y: -newOffset,
            width: view.bounds.width, height: contentHeight)
    }

    /// Clamp a raw offset to `[0, maxOffsetY]` and report whether the
    /// raw value overflowed (used by `MomentumState.advance` to know
    /// when to stop).
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
                momentum = MomentumState(
                    velocityPxPerSec: velocity, lastTick: CACurrentMediaTime())
                updateDisplayLink()
            } else if isAtBottomEdge() {
                scrollPosition = .followsBottom
            }
        default:
            break
        }
    }

    func stopMomentum() { momentum = nil }

    func advanceMomentum(now: CFTimeInterval) {
        guard var state = momentum else { return }
        let (newOffset, done) = MomentumState.advance(
            offset: contentOffsetY, state: &state, now: now,
            clamp: { [weak self] candidate in
                self?.clampOffset(candidate) ?? (candidate, false)
            })
        setContentOffsetY(newOffset)
        if done {
            momentum = nil
            scrollPosition = isAtBottomEdge() ? .followsBottom : .fixedAt(newOffset)
        } else {
            momentum = state
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
