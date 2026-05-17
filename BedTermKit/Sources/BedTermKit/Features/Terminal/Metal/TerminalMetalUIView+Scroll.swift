import UIKit

/// Touch-scroll for the Metal terminal. Drag tracks the finger 1:1; release
/// with velocity decelerates and snaps to whole rows; tap (handled by the
/// focus-tap recognizer in the main file) stops inertia immediately.
///
/// Per R5.scroll_inertia, R5.scroll, and R5.scroll_sync (mvp.xml). The
/// authoritative scroll offset lives in the Rust core (`TerminalCore`); these
/// methods only translate gesture input into `scrollBy(_:)` calls.
extension TerminalMetalUIView {

    @objc func handlePan(_ gesture: UIPanGestureRecognizer) {
        switch gesture.state {
        case .began:
            stopInertia()
            dragAccumulator = 0
        case .changed:
            let translation = gesture.translation(in: self).y
            gesture.setTranslation(.zero, in: self)
            // iOS natural-scroll: the content tracks the finger. Dragging
            // the finger downward (positive y) reveals content above the
            // viewport — i.e. scrolls into history. `points` is signed so
            // a positive value scrolls toward older content (offset++).
            applyScroll(points: translation)
        case .ended, .cancelled:
            let velocity = gesture.velocity(in: self).y
            if abs(velocity) > 50 { startInertia(initialVelocity: velocity) }
        default:
            break
        }
    }

    /// Translate a "+up" point delta into whole-row scrolls, retaining the
    /// sub-row remainder for the next call.
    private func applyScroll(points: CGFloat) {
        guard cellSize.height > 0 else { return }
        dragAccumulator += points
        let rows = Int((dragAccumulator / cellSize.height).rounded(.towardZero))
        if rows != 0 {
            dragAccumulator -= CGFloat(rows) * cellSize.height
            terminalCore.scrollBy(rows)
            setNeedsDisplay()
        }
    }

    private func startInertia(initialVelocity: CGFloat) {
        stopInertia()
        inertiaVelocity = initialVelocity
        let link = CADisplayLink(target: self, selector: #selector(tickInertia(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    @objc func tickInertia(_ link: CADisplayLink) {
        let dt = CGFloat(link.targetTimestamp - link.timestamp)
        guard dt > 0 else { return }
        applyScroll(points: inertiaVelocity * dt)
        // Exponential decay: ~0.1% of original velocity remains after one
        // second (pow(0.001, 1) == 0.001). A typical flick (~2000 pt/s)
        // settles visually in ~600 ms.
        inertiaVelocity *= pow(0.001, dt)

        let offset = terminalCore.scrollOffset
        let atTop = offset >= terminalCore.scrollbackLines
        let atBottom = offset == 0
        let exhausted = abs(inertiaVelocity) < 30
        let pinnedAtEdge = (inertiaVelocity > 0 && atTop) || (inertiaVelocity < 0 && atBottom)
        if exhausted || pinnedAtEdge {
            stopInertia()
        }
    }

    func stopInertia() {
        displayLink?.invalidate()
        displayLink = nil
        inertiaVelocity = 0
    }
}
