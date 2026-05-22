import BedTermCoreC
import UIKit

/// Layout helpers split off the main controller for SwiftLint
/// `file_length`. M4+ owns building the per-frame `BtBlockHeaderEntry`
/// table; per-block UIHostingController mounts and divider UIViews
/// are gone — Rust paints both.
@MainActor
extension BlockListContainerViewController {
    /// Build per-block header descriptors. UTF-8 bytes are returned in
    /// the parallel `storage` array; the Metal view patches pointers
    /// at the FFI call site (pointers can't survive across the call
    /// otherwise). Sticky descriptors are added later by
    /// `BlockListContainerView+Sticky` (M5); this method handles only
    /// natural in-flow headers.
    func buildHeaderDescriptors(
        ranges: [BlockRange],
        scale: Float,
        widthPx: Float
    ) -> ([BtBlockHeaderEntry], [(command: Data, subtitle: Data?)]) {
        let traits = view.traitCollection
        let bg =
            (UIColor(named: "ShadcnBackground", in: .module, compatibleWith: traits)
            ?? UIColor.systemBackground).asRGBA32()
        let fg =
            (UIColor(named: "ShadcnPrimary", in: .module, compatibleWith: traits)
            ?? UIColor.label).asRGBA32()
        let muted =
            (UIColor(named: "ShadcnMutedForeground", in: .module, compatibleWith: traits)
            ?? UIColor.secondaryLabel).asRGBA32()
        let divider = resolveDividerColor().asRGBA32()
        let headerHpx = Float(headerHeightPt) * scale

        var entries: [BtBlockHeaderEntry] = []
        var storage: [(command: Data, subtitle: Data?)] = []
        entries.reserveCapacity(ranges.count)
        storage.reserveCapacity(ranges.count)

        for (idx, range) in ranges.enumerated() {
            let block = range.block
            let cmdData =
                BlockHeaderModel.displayCommand(for: block)
                .data(using: .utf8) ?? Data()
            let subData = BlockHeaderModel.subtitle(for: block)?.data(using: .utf8)
            storage.append((cmdData, subData))

            let yTopPx = Float(range.top) * scale
            entries.append(
                BtBlockHeaderEntry(
                    block_id: block.id,
                    header_y_top_px: yTopPx,
                    header_height_px: headerHpx,
                    panel_x_left_px: 0,
                    panel_width_px: widthPx,
                    command_utf8: nil,
                    command_len: UInt32(cmdData.count),
                    subtitle_utf8: nil,
                    subtitle_len: UInt32(subData?.count ?? 0),
                    agent_id: BlockHeaderModel.agentID(block.cliAgent),
                    _pad: (0, 0, 0),
                    badge_tint_rgba: BlockHeaderModel.tint(for: block.cliAgent),
                    header_bg_rgba: bg,
                    command_fg_rgba: fg,
                    subtitle_fg_rgba: muted,
                    divider_rgba: idx == 0 ? 0 : divider,
                    is_sticky: 0,
                    _pad2: (0, 0, 0),
                    body_clip_y_top_px: yTopPx + headerHpx,
                    body_clip_height_px: Float(range.bot - range.top) * scale - headerHpx
                ))
        }
        return (entries, storage)
    }

    func resolveDividerColor() -> UIColor {
        UIColor(named: "ShadcnBorder", in: .module, compatibleWith: view.traitCollection)?
            .withAlphaComponent(0.5) ?? UIColor.separator
    }
}
