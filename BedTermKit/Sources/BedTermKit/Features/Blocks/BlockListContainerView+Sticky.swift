import BedTermCoreC
import UIKit

/// Section-style header pinning for the block list, rendered as part of
/// the single Metal pass. We compute the pinned screen-space Y for the
/// currently-scrolled block here, build a `BtBlockHeaderEntry` with
/// `is_sticky = 1`, and the renderer paints it on top of everything
/// else (z-order via second iteration in `Renderer::draw_block_list`).
///
/// When a sticky entry is emitted, the same block's *natural* in-flow
/// header descriptor is omitted from the headers array so the band
/// doesn't double-draw.
/// Output of `buildStickyDescriptor`. Wraps three fields rather than
/// a bare tuple so SwiftLint's `large_tuple` rule doesn't complain.
struct StickyHeaderDescriptor {
    let entry: BtBlockHeaderEntry
    let blockID: UInt64
    let storage: (command: Data, subtitle: Data?)
}

@MainActor
extension BlockListContainerViewController {
    /// Index of the block whose header should be pinned, or `nil` when
    /// no block intersects the sticky band (scrolled above the first
    /// block, or the list is empty).
    func stickyActiveIndex(ranges: [BlockRange], scrollY: CGFloat) -> Int? {
        guard !ranges.isEmpty else { return nil }
        var found: Int?
        for (idx, range) in ranges.enumerated() where range.top <= scrollY && range.bot > scrollY {
            found = idx
        }
        return found
    }

    /// Build a sticky header descriptor for the currently pinned block,
    /// or `nil` when no sticky band is needed. Returned tuple carries
    /// the same `(command, subtitle)` storage tuple the natural-header
    /// path uses so the FFI call site sees a uniform interface.
    func buildStickyDescriptor(
        ranges: [BlockRange],
        scrollY: CGFloat,
        scale: Float,
        widthPx: Float
    ) -> StickyHeaderDescriptor? {
        guard let active = stickyActiveIndex(ranges: ranges, scrollY: scrollY) else { return nil }
        let range = ranges[active]
        let nextTop = (active + 1 < ranges.count) ? ranges[active + 1].top : .infinity

        // Screen-space pinned Y. naturalScreenY is where the in-flow
        // header would sit; clamp at 0 to "pin" it once it scrolls past
        // the top, then push back down once the next block's natural
        // header has entered the top band (handoff).
        let naturalScreenY = range.top - scrollY
        let pushUpLimit = (nextTop - scrollY) - headerHeightPt
        let pinnedY = min(max(naturalScreenY, 0), pushUpLimit)

        let traits = view.traitCollection
        func token(_ name: String, fallback: UIColor) -> UInt32 {
            let raw = UIColor(named: name, in: .module, compatibleWith: traits) ?? fallback
            return raw.resolvedColor(with: traits).asRGBA32()
        }
        let fg = token("ShadcnPrimary", fallback: .label)
        let muted = token("ShadcnMutedForeground", fallback: .secondaryLabel)
        let divider = resolveDividerColor().resolvedColor(with: traits).asRGBA32()

        let cmdData =
            BlockHeaderModel.displayCommand(for: range.block)
            .data(using: .utf8) ?? Data()
        let subData = BlockHeaderModel.subtitle(for: range.block)?.data(using: .utf8)

        let entry = BtBlockHeaderEntry(
            block_id: range.block.id,
            header_y_top_px: Float(pinnedY) * scale,
            header_height_px: Float(headerHeightPt) * scale,
            panel_x_left_px: 0,
            panel_width_px: widthPx,
            command_utf8: nil,
            command_len: UInt32(cmdData.count),
            subtitle_utf8: nil,
            subtitle_len: UInt32(subData?.count ?? 0),
            agent_id: BlockHeaderModel.agentID(range.block.cliAgent),
            _pad: (0, 0, 0),
            badge_tint_rgba: BlockHeaderModel.tint(for: range.block.cliAgent),
            command_fg_rgba: fg,
            subtitle_fg_rgba: muted,
            divider_rgba: divider,
            is_sticky: 1,
            _pad2: (0, 0, 0))
        return StickyHeaderDescriptor(
            entry: entry,
            blockID: range.block.id,
            storage: (cmdData, subData))
    }
}
