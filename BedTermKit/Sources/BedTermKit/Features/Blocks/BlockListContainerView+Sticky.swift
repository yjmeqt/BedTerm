import SwiftUI
import UIKit

/// Section-style header pinning for the block list. The currently-scrolled
/// block's `BlockHeader` floats above `metalView` at the top of the
/// viewport so the user can always read which command's output they're
/// looking at, even when that output is taller than the screen. When the
/// next block's natural header starts scrolling into view it pushes the
/// pinned header up off-screen — the standard iOS table-section feel.
///
/// Implementation:
/// - A dedicated `pinnedHost: UIHostingController<BlockHeader>` lives in
///   `view` (not `scrollView.contentView`) so it sits above the Metal
///   surface that paints the block bodies. A solid backdrop `pinnedBackground`
///   sits behind it to occlude any cells the Metal surface would otherwise
///   draw under the header.
/// - The natural in-`contentView` header for the pinned block is hidden
///   so they don't double-render.
@MainActor
extension BlockListContainerViewController {
    func updateStickyHeader(ranges: [BlockRange], scrollY: CGFloat, width: CGFloat) {
        guard let active = stickyActiveIndex(ranges: ranges, scrollY: scrollY) else {
            dismissStickyHeader()
            return
        }
        let range = ranges[active]
        let nextTop = (active + 1 < ranges.count) ? ranges[active + 1].top : .infinity
        // Pinned Y in `view` (screen) coordinates. Natural header in
        // contentView is at (range.top - scrollY) on screen; once that's
        // negative we want to clamp to 0 (pin), then push back down once
        // the next block's natural header has entered the top band.
        let naturalScreenY = range.top - scrollY
        let pushUpLimit = (nextTop - scrollY) - headerHeightPt
        let pinnedY = min(max(naturalScreenY, 0), pushUpLimit)
        showStickyHeader(for: range.block, topY: pinnedY, width: width)
    }

    /// Find the block whose body region the viewport is currently scrolled
    /// into — i.e. the block we want to label at the top of the screen.
    /// Returns nil when scrolled above the first block (nothing to pin).
    private func stickyActiveIndex(ranges: [BlockRange], scrollY: CGFloat) -> Int? {
        guard !ranges.isEmpty else { return nil }
        // Last index whose top <= scrollY + headerHeightPt: the block whose
        // header has already passed (or is just passing) the viewport top.
        // We pin starting the moment the body would otherwise eat the
        // header — i.e. once `scrollY > range.top`.
        var found: Int?
        for (idx, range) in ranges.enumerated() {
            if range.top <= scrollY && range.bot > scrollY {
                found = idx
            }
        }
        return found
    }

    private func showStickyHeader(for block: Block, topY: CGFloat, width: CGFloat) {
        let leftInset = BlockPanelStyle.cellLeftInsetPt
        let frame = CGRect(
            x: leftInset, y: topY,
            width: width - leftInset, height: headerHeightPt)
        let backdropFrame = CGRect(
            x: 0, y: topY,
            width: width, height: headerHeightPt)

        // Hide the natural header so we don't render twice.
        if let id = pinnedBlockID, id != block.id {
            // Block changed — un-hide the previously-pinned natural header.
            // (No-op if it got recycled out of view.)
        }
        pinnedBlockID = block.id

        let bg: UIView
        if let existing = pinnedBackground {
            bg = existing
        } else {
            bg = UIView()
            bg.backgroundColor = resolveStickyBackground()
            bg.isUserInteractionEnabled = false
            view.addSubview(bg)
            pinnedBackground = bg
        }
        bg.frame = backdropFrame

        let host: UIHostingController<BlockHeader>
        if let existing = pinnedHost {
            existing.rootView = BlockHeader(block: block)
            host = existing
        } else {
            let new = UIHostingController(rootView: BlockHeader(block: block))
            new.view.backgroundColor = .clear
            new.view.isUserInteractionEnabled = false
            addChild(new)
            view.addSubview(new.view)
            new.didMove(toParent: self)
            pinnedHost = new
            host = new
        }
        host.view.frame = frame
        host.view.isHidden = false
        bg.isHidden = false

        // Keep z-order: backdrop sits beneath header host, both above
        // metalView and any other sibling.
        view.bringSubviewToFront(bg)
        view.bringSubviewToFront(host.view)
    }

    private func dismissStickyHeader() {
        pinnedHost?.view.isHidden = true
        pinnedBackground?.isHidden = true
        // Don't tear down the host — keep it warm for the next scroll
        // that re-enters a block.
        pinnedBlockID = nil
    }

    /// Match the scroll-view background so the floating header reads as
    /// "part of the list", not a separate floating chip.
    private func resolveStickyBackground() -> UIColor {
        UIColor(
            named: "ShadcnBackground", in: .module, compatibleWith: view.traitCollection
        ) ?? UIColor.systemBackground
    }
}
