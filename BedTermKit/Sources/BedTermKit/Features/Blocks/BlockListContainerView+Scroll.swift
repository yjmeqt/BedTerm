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
/// Subsequent content growth must NOT yank the viewport — we just keep
/// the recorded offset. Transition back to `.followsBottom` when the
/// user explicitly scrolls to the bottom edge.
enum ScrollPosition: Equatable {
    case followsBottom
    case fixedAt(CGFloat)
}

/// UIScrollViewDelegate hooks and scroll-anchor transitions. Split
/// off the main container for file_length. Mirrors Warp's
/// `ScrollPositionUpdate` event flow at the UIKit gesture level:
/// drag begins → capture anchor; drag/inertia ends at the bottom →
/// resume "follows bottom".
@MainActor
extension BlockListContainerViewController {
    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        // Both updates: visibility band moved (new hosts may enter the
        // overscan window; old hosts may leave), and the Metal pane
        // needs the new scroll offset before its next draw.
        syncHeaders()
        pushLayoutToMetalView()
    }

    /// User started a drag. Capture current offset as the explicit
    /// anchor — from this moment until they land back at the bottom
    /// edge, content growth must not yank the viewport. Mirrors how
    /// Warp transitions `FollowsBottomOfMostRecentBlock` →
    /// `FixedAtPosition` on `AfterScrollEvent`.
    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        scrollPosition = .fixedAt(scrollView.contentOffset.y)
    }

    /// Drag finished with no momentum. If the user came to rest at the
    /// bottom, they want to follow live output again; otherwise stay
    /// anchored.
    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate { reanchorAfterScrollEnd() }
    }

    /// Inertia finished. Same re-evaluation as `didEndDragging` for
    /// the momentum-scroll case.
    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        reanchorAfterScrollEnd()
    }

    private func reanchorAfterScrollEnd() {
        if isAtBottomEdge() {
            scrollPosition = .followsBottom
        } else {
            scrollPosition = .fixedAt(scrollView.contentOffset.y)
        }
    }
}
