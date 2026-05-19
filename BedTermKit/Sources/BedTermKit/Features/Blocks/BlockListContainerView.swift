import BedTermCoreC
import SwiftUI
import UIKit

/// UIKit composition: UIScrollView wraps a transparent content view
/// holding per-block SwiftUI header strips at absolute Y offsets;
/// TerminalBlocksMetalView is pinned sibling-on-top, paints all
/// visible block BODIES via one Metal surface using the layout table
/// the container computes.
@MainActor
final class BlockListContainerView: UIView, UIScrollViewDelegate {
    private let session: TerminalSession
    private let scrollView = UIScrollView()
    private let contentView = UIView()
    private let metalView: TerminalBlocksMetalView
    private var headerHosts: [UInt64: UIHostingController<BlockHeader>] = [:]

    /// Fixed SwiftUI header strip height. Matches the BlockHeader's
    /// natural height with current padding (subheadline + caption2 + 10pt
    /// vertical padding ≈ 56pt). v1 limitation: may clip at Dynamic Type
    /// XXL — acceptable per Phase B plan.
    private let headerHeightPt: CGFloat = 56

    /// Cell row height in points, resolved from the shared renderer's
    /// atlas metrics. Fallback to 18 if metrics unavailable.
    private var rowHeightPt: CGFloat = 18

    init(session: TerminalSession) {
        self.session = session
        self.metalView = TerminalBlocksMetalView(session: session)
        super.init(frame: .zero)

        scrollView.delegate = self
        scrollView.alwaysBounceVertical = true
        scrollView.backgroundColor = .clear
        contentView.backgroundColor = .clear
        addSubview(scrollView)
        scrollView.addSubview(contentView)
        addSubview(metalView)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used") }

    /// Called by the SwiftUI wrapper's `updateUIView` whenever observed
    /// state changes (BlockStore.blocks). Rebuilds header hosts, content
    /// size, and the Metal layout table.
    func refresh() {
        refreshRowHeight()
        rebuildHeaders()
        updateContentSize()
        pushLayoutToMetalView()
    }

    private func refreshRowHeight() {
        let scale = window?.screen.scale ?? 3.0
        let metrics = MetalEnvironment.shared.renderer.cellSizeInPoints(scale: scale)
        if metrics.height >= 8 && metrics.height <= 64 {
            rowHeightPt = metrics.height
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        scrollView.frame = bounds
        metalView.frame = bounds
        refresh()
    }

    private func bodyHeightPt(for block: Block) -> CGFloat {
        let rows: Int = {
            if let end = block.endLine {
                return max(1, Int(end - block.startLine))
            }
            if let core = session.terminalCore {
                return max(1, Int(core.currentLine + 1 - block.startLine))
            }
            return 1
        }()
        return CGFloat(rows) * rowHeightPt
    }

    private func updateContentSize() {
        let blocks = session.blockStore.blocks
        var total: CGFloat = 0
        for block in blocks {
            total += headerHeightPt
            total += bodyHeightPt(for: block)
        }
        scrollView.contentSize = CGSize(width: bounds.width, height: total)
        contentView.frame = CGRect(origin: .zero, size: scrollView.contentSize)
    }

    private func rebuildHeaders() {
        let blocks = session.blockStore.blocks
        let liveIDs = Set(blocks.map(\.id))
        // Recycle hosts: drop any whose block is gone.
        for (id, host) in headerHosts where !liveIDs.contains(id) {
            host.view.removeFromSuperview()
            host.willMove(toParent: nil)
            host.removeFromParent()
            headerHosts.removeValue(forKey: id)
        }
        var yPt: CGFloat = 0
        for block in blocks {
            let host: UIHostingController<BlockHeader>
            if let existing = headerHosts[block.id] {
                existing.rootView = BlockHeader(block: block)
                host = existing
            } else {
                host = UIHostingController(rootView: BlockHeader(block: block))
                host.view.backgroundColor = .clear
                headerHosts[block.id] = host
            }
            host.view.frame = CGRect(
                x: 0, y: yPt, width: bounds.width, height: headerHeightPt)
            if host.view.superview == nil {
                contentView.addSubview(host.view)
            }
            yPt += headerHeightPt
            yPt += bodyHeightPt(for: block)
        }
    }

    private func pushLayoutToMetalView() {
        let blocks = session.blockStore.blocks
        let scale = window?.screen.scale ?? 3.0
        var yPt: CGFloat = 0
        var entries: [BtBlockLayoutEntry] = []
        entries.reserveCapacity(blocks.count)
        for block in blocks {
            yPt += headerHeightPt
            let bodyPt = bodyHeightPt(for: block)
            entries.append(
                BtBlockLayoutEntry(
                    block_id: block.id,
                    body_y_top_px: Float(yPt * scale),
                    body_height_px: Float(bodyPt * scale)
                ))
            yPt += bodyPt
        }
        metalView.update(scrollOffset: scrollView.contentOffset.y, layout: entries)
    }

    // MARK: - UIScrollViewDelegate

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        pushLayoutToMetalView()
    }
}
