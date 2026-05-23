import BedTermCoreC
import CoreGraphics

/// Block-list layout payload, pushed per-frame by the block-list
/// interaction controller into the shared `TerminalMetalUIView`.
struct BlockLayoutState {
    var scrollOffsetPx: CGFloat = 0
    var entries: [BtBlockLayoutEntry] = []
    var headers: [BtBlockHeaderEntry] = []
    var headerBlob: [UInt8] = []
}

extension TerminalMetalUIView {
    /// Called by `BlockListContainerViewController` before each redraw.
    /// `headers` must have their UTF-8 pointers already patched into
    /// `headerBlob` — the blob is retained in `blockLayout` so pointers
    /// stay live through `draw(_:)`.
    func updateBlockLayout(
        scrollOffsetPx: CGFloat,
        entries: [BtBlockLayoutEntry],
        headers: [BtBlockHeaderEntry],
        headerBlob: [UInt8]
    ) {
        blockLayout = BlockLayoutState(
            scrollOffsetPx: scrollOffsetPx,
            entries: entries,
            headers: headers,
            headerBlob: headerBlob)
    }
}
