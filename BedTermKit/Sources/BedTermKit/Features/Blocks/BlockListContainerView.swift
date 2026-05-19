import BedTermCoreC
import SwiftUI
import UIKit

/// UIKit composition for the Phase B single Metal surface block list.
/// Implemented as a `UIViewController` so per-block SwiftUI header strips
/// (hosted via `UIHostingController`) get proper child-VC lifecycle
/// (`addChild` / `didMove(toParent:)`) — that's what propagates trait
/// changes, Dynamic Type, VoiceOver focus, and responder chain to the
/// embedded SwiftUI.
///
/// The view hosts:
///   - `scrollView` (manages momentum + contentSize),
///   - `contentView` inside scrollView holding the per-block header
///     strips at absolute Y,
///   - `metalView` pinned sibling on top, paints all visible block
///     bodies via one `bt_renderer_draw_block_list` call per frame.
@MainActor
final class BlockListContainerViewController: UIViewController, UIScrollViewDelegate {
    private let session: TerminalSession
    private let scrollView = UIScrollView()
    private let contentView = UIView()
    private let metalView: TerminalBlocksMetalView
    private var headerHosts: [UInt64: UIHostingController<BlockHeader>] = [:]

    // Layout constants. headerHeightPt may clip at Dynamic Type XXL —
    // accepted v1 limitation per Phase B plan.
    private let headerHeightPt: CGFloat = 56
    private let pinTolerancePt: CGFloat = 24
    private let headerOverscanPt: CGFloat = 200

    // Atlas-derived metrics; refreshed from the shared renderer each
    // layout pass. Fallbacks cover the early-launch race where atlas
    // metrics aren't yet available.
    private var rowHeightPt: CGFloat = 18
    private var cellWidthPt: CGFloat = 9

    private var displayLink: CADisplayLink?
    private var isPushingLayout = false
    private var lastLayoutBounds: CGSize = .zero

    // Selection lives on a dedicated controller; the container provides
    // its hit-resolver + text-resolver since both walk the same block
    // cumulative-Y the layout pipeline already computes.
    private var selection: BlockListSelectionController!

    init(session: TerminalSession) {
        self.session = session
        self.metalView = TerminalBlocksMetalView(session: session)
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used") }

    override func loadView() {
        let root = UIView()
        root.backgroundColor = .clear
        view = root
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        scrollView.delegate = self
        // Warp-style: no rubber-band overscroll. Top/bottom clamp hard.
        scrollView.bounces = false
        scrollView.alwaysBounceVertical = false
        scrollView.backgroundColor = .clear
        contentView.backgroundColor = .clear
        view.addSubview(scrollView)
        scrollView.addSubview(contentView)
        view.addSubview(metalView)
        selection = BlockListSelectionController(
            contentView: contentView,
            headerHeightPt: headerHeightPt,
            hitResolver: { [weak self] point in self?.blockHitTest(at: point) },
            textResolver: { [weak self] id, range in
                self?.extractText(blockID: id, range: range)
            })
        view.addGestureRecognizer(selection.longPressGR)
    }

    /// Called explicitly by the SwiftUI representable's
    /// `dismantleUIViewController` so the CADisplayLink retain cycle
    /// (link → self) is broken before the controller drops out of the
    /// SwiftUI hierarchy. Cannot rely on deinit — the link IS what would
    /// be keeping us alive.
    func teardown() {
        displayLink?.invalidate()
        displayLink = nil
        for (_, host) in headerHosts {
            host.willMove(toParent: nil)
            host.view.removeFromSuperview()
            host.removeFromParent()
        }
        headerHosts.removeAll()
        scrollView.delegate = nil
    }

    /// Called by the SwiftUI wrapper's `updateUIViewController` whenever
    /// observed state changes (BlockStore.blocks). Rebuilds header hosts,
    /// content size, and the Metal layout table; preserves pin-to-bottom
    /// when content grew.
    func refresh() {
        refreshRowHeight()
        let wasPinnedToBottom = isPinnedToBottom()
        updateContentSize()
        if wasPinnedToBottom {
            scrollToBottom(animated: false)
        }
        // syncHeaders depends on the up-to-date contentOffset so it
        // runs AFTER the pin-to-bottom adjustment, not before.
        syncHeaders()
        pushLayoutToMetalView()
        updateDisplayLink()
    }

    private func isPinnedToBottom() -> Bool {
        let maxOffset = max(0, scrollView.contentSize.height - scrollView.bounds.height)
        return scrollView.contentOffset.y >= maxOffset - pinTolerancePt
    }

    private func scrollToBottom(animated: Bool) {
        let maxOffset = max(0, scrollView.contentSize.height - scrollView.bounds.height)
        guard scrollView.bounds.height > 0 else { return }
        scrollView.setContentOffset(CGPoint(x: 0, y: maxOffset), animated: animated)
    }

    private func updateDisplayLink() {
        let hasRunning = session.blockStore.blocks.contains(where: \.isRunning)
        if hasRunning {
            if displayLink == nil {
                let link = CADisplayLink(target: self, selector: #selector(handleDisplayTick))
                link.add(to: .main, forMode: .common)
                displayLink = link
            }
        } else {
            displayLink?.invalidate()
            displayLink = nil
        }
    }

    @objc private func handleDisplayTick() {
        let wasPinnedToBottom = isPinnedToBottom()
        updateContentSize()
        if wasPinnedToBottom {
            scrollToBottom(animated: false)
        }
        syncHeaders()
        pushLayoutToMetalView()
    }

    private func refreshRowHeight() {
        let scale = view.window?.screen.scale ?? 3.0
        let metrics = MetalEnvironment.shared.renderer.cellSizeInPoints(scale: scale)
        if metrics.height >= 8 && metrics.height <= 64 {
            rowHeightPt = metrics.height
        }
        if metrics.width >= 4 && metrics.width <= 32 {
            cellWidthPt = metrics.width
        }
        selection?.updateMetrics(
            cellWidth: cellWidthPt, rowHeight: rowHeightPt,
            containerWidth: view.bounds.width,
            leftInset: BlockPanelStyle.cellLeftInsetPt)
    }

    override func viewWillLayoutSubviews() {
        super.viewWillLayoutSubviews()
        let bounds = view.bounds
        scrollView.frame = bounds
        // Inset the Metal surface so cell column 0 starts inside the
        // Warp panel chrome (left padding); Rust paints the rounded
        // panel BG behind the inset region.
        let leftInset = BlockPanelStyle.cellLeftInsetPt
        metalView.frame = CGRect(
            x: leftInset, y: 0,
            width: max(0, bounds.width - leftInset),
            height: bounds.height)
        // Only re-run the heavy refresh path when bounds actually
        // change. layoutSubviews fires repeatedly during scroll /
        // rotation / keyboard transitions; rebuildHeaders() is O(N).
        if bounds.size != lastLayoutBounds {
            lastLayoutBounds = bounds.size
            refresh()
        }
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
        let panelInsetPt: CGFloat = 4
        var total: CGFloat = 0
        for block in blocks {
            total += headerHeightPt + bodyHeightPt(for: block) + panelInsetPt
        }
        let width = view.bounds.width
        scrollView.contentSize = CGSize(width: width, height: total)
        contentView.frame = CGRect(origin: .zero, size: scrollView.contentSize)
    }

    /// Mount UIHostingControllers only for blocks whose vertical range
    /// intersects the visible viewport ± `headerOverscanPt`. With long
    /// sessions (hundreds of blocks) this caps the active host count at
    /// roughly the visible block count instead of growing unbounded.
    private func syncHeaders() {
        let blocks = session.blockStore.blocks
        let width = view.bounds.width
        let leftInset = BlockPanelStyle.cellLeftInsetPt
        let scrollY = scrollView.contentOffset.y
        let viewportTop = scrollY - headerOverscanPt
        let viewportBot = scrollY + scrollView.bounds.height + headerOverscanPt

        var keepIDs = Set<UInt64>()
        keepIDs.reserveCapacity(blocks.count)
        var yPt: CGFloat = 0
        let panelInsetPt: CGFloat = 4
        for block in blocks {
            let blockTop = yPt
            let blockBot = blockTop + headerHeightPt + bodyHeightPt(for: block)
            let intersects = blockBot >= viewportTop && blockTop <= viewportBot
            if intersects {
                keepIDs.insert(block.id)
                let host: UIHostingController<BlockHeader>
                if let existing = headerHosts[block.id] {
                    existing.rootView = BlockHeader(block: block)
                    host = existing
                } else {
                    host = UIHostingController(rootView: BlockHeader(block: block))
                    host.view.backgroundColor = .clear
                    headerHosts[block.id] = host
                    addChild(host)
                    contentView.addSubview(host.view)
                    host.didMove(toParent: self)
                }
                host.view.frame = CGRect(
                    x: leftInset, y: blockTop,
                    width: width - leftInset, height: headerHeightPt)
            }
            yPt = blockBot + panelInsetPt
        }

        for (id, host) in headerHosts where !keepIDs.contains(id) {
            host.willMove(toParent: nil)
            host.view.removeFromSuperview()
            host.removeFromParent()
            headerHosts.removeValue(forKey: id)
        }
    }

    private func pushLayoutToMetalView() {
        // Reentrancy guard: scrollToBottom() triggers
        // scrollViewDidScroll → pushLayoutToMetalView.
        guard !isPushingLayout else { return }
        isPushingLayout = true
        defer { isPushingLayout = false }

        let blocks = session.blockStore.blocks
        let scale = view.window?.screen.scale ?? 3.0
        let panelInsetPt: CGFloat = 4  // small gap between panels
        let panelBgRgba = BlockPanelStyle.bgRgba(for: view.traitCollection)
        let cornerRadiusPx = Float(BlockPanelStyle.cornerRadiusPt * scale)
        let metalViewWidth = metalView.bounds.width
        var yPt: CGFloat = 0
        var entries: [BtBlockLayoutEntry] = []
        entries.reserveCapacity(blocks.count)
        for block in blocks {
            let panelTop = yPt + panelInsetPt / 2
            let bodyTop = yPt + headerHeightPt
            let bodyPt = bodyHeightPt(for: block)
            let panelBot = bodyTop + bodyPt + panelInsetPt / 2
            entries.append(
                BtBlockLayoutEntry(
                    block_id: block.id,
                    body_y_top_px: Float(bodyTop * scale),
                    body_height_px: Float(bodyPt * scale),
                    panel_y_top_px: Float(panelTop * scale),
                    panel_height_px: Float((panelBot - panelTop) * scale),
                    panel_x_left_px: 0,
                    panel_width_px: Float(metalViewWidth * scale),
                    panel_bg_rgba: panelBgRgba,
                    panel_corner_radius_px: cornerRadiusPx
                ))
            yPt += headerHeightPt + bodyPt + panelInsetPt
        }
        metalView.update(scrollOffset: scrollView.contentOffset.y, layout: entries)
    }

    // MARK: - Selection resolvers

    private func blockHitTest(at point: CGPoint) -> BlockListSelectionController.BlockHit? {
        guard let core = session.terminalCore else { return nil }
        var yPt: CGFloat = 0
        for block in session.blockStore.blocks {
            let bodyTop = yPt + headerHeightPt
            let bodyBot = bodyTop + bodyHeightPt(for: block)
            if point.y >= bodyTop && point.y < bodyBot {
                guard let snap = snapshot(for: block, core: core) else { return nil }
                return .init(
                    blockID: block.id, bodyTop: bodyTop,
                    rows: Int(snap.rows), cols: Int(snap.cols))
            }
            yPt = bodyBot
        }
        return nil
    }

    private func snapshot(for block: Block, core: TerminalCore) -> GridSnapshot? {
        if block.hasFrozenSnapshot {
            let frozenIdx = core.allBlocks().firstIndex(where: { $0.id == block.id })
            if let idx = frozenIdx { return core.frozenSnapshot(forBlockAt: idx) }
        }
        if block.isRunning {
            let end = core.currentLine + 1
            if end > block.startLine {
                return core.snapshotRange(startLine: block.startLine, endLine: end)
            }
        }
        return nil
    }

    private func extractText(blockID: UInt64, range: SelectionRange) -> String? {
        guard let core = session.terminalCore,
            let block = session.blockStore.blocks.first(where: { $0.id == blockID }),
            let snap = snapshot(for: block, core: core)
        else { return nil }
        let norm = range.normalised
        let rowsCount = Int(snap.rows)
        let colsCount = Int(snap.cols)
        let lastRow = min(norm.endRow, rowsCount - 1)
        guard norm.startRow <= lastRow else { return nil }
        var out = ""
        for row in norm.startRow...lastRow {
            let from = (row == norm.startRow) ? norm.startCol : 0
            let to = (row == norm.endRow) ? norm.endCol : colsCount
            for col in from..<min(to, colsCount) {
                let scalar = snap.cell(col: col, row: row).flatMap { cell in
                    cell.ch != 0 ? Unicode.Scalar(cell.ch) : nil
                }
                out.append(scalar.map(Character.init) ?? " ")
            }
            if row != lastRow { out.append("\n") }
        }
        return out
    }

    // MARK: - UIScrollViewDelegate

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        // Both updates: visibility band moved (new hosts may enter the
        // overscan window; old hosts may leave), and the Metal pane
        // needs the new scroll offset before its next draw.
        syncHeaders()
        pushLayoutToMetalView()
    }
}
