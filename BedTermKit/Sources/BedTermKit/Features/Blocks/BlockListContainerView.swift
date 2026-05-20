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
    let session: TerminalSession
    private let scrollView = UIScrollView()
    let contentView = UIView()
    private let metalView: TerminalBlocksMetalView
    var headerHosts: [UInt64: UIHostingController<BlockHeader>] = [:]
    /// Thin hairline UIViews drawn in the gap above each block (skipped
    /// for the first block). Live in `contentView` so they scroll with
    /// the list; sit beneath `metalView` but show through where the
    /// metal surface has no body content to paint (transparent BG).
    var dividerHosts: [UInt64: UIView] = [:]

    /// Section-header pinning: a single hosting controller floats above
    /// `metalView` (in `view`, not `contentView`) and adopts the
    /// currently-scrolling block's command/header content. Lets the user
    /// see which command they're inside even when its output is taller
    /// than the screen. Lazily created the first time a header needs to
    /// be pinned; `pinnedBlockID` tracks which block is currently shown
    /// (or nil when no block intersects the sticky band).
    var pinnedHost: UIHostingController<BlockHeader>?
    var pinnedBlockID: UInt64?
    /// Solid backdrop strip behind `pinnedHost` so the floating header
    /// occludes the body cells `metalView` paints beneath it.
    var pinnedBackground: UIView?

    // Layout constants. headerHeightPt may clip at Dynamic Type XXL —
    // accepted v1 limitation per Phase B plan.
    let headerHeightPt: CGFloat = 56
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
        for (_, divider) in dividerHosts { divider.removeFromSuperview() }
        dividerHosts.removeAll()
        if let host = pinnedHost {
            host.willMove(toParent: nil)
            host.view.removeFromSuperview()
            host.removeFromParent()
        }
        pinnedHost = nil
        pinnedBackground?.removeFromSuperview()
        pinnedBackground = nil
        pinnedBlockID = nil
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

    func bodyHeightPt(for block: Block) -> CGFloat {
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
        let gap = BlockPanelStyle.interBlockGapPt
        var total: CGFloat = 0
        for block in blocks {
            total += headerHeightPt + bodyHeightPt(for: block) + gap
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
        let scrollY = scrollView.contentOffset.y
        let viewportTop = scrollY - headerOverscanPt
        let viewportBot = scrollY + scrollView.bounds.height + headerOverscanPt
        let gap = BlockPanelStyle.interBlockGapPt
        let ranges = computeBlockRanges(blocks: blocks, gap: gap)

        var keepIDs = Set<UInt64>()
        keepIDs.reserveCapacity(blocks.count)
        let dividerColor = resolveDividerColor()
        for (idx, range) in ranges.enumerated() {
            let intersects = range.bot >= viewportTop && range.top <= viewportBot
            if intersects {
                keepIDs.insert(range.block.id)
                mountHeader(for: range.block, blockTop: range.top, width: width)
                placeDividerForRange(range, idx: idx, width: width, gap: gap, color: dividerColor)
            }
        }
        recycleHostsAndDividers(keep: keepIDs)
        updateStickyHeader(ranges: ranges, scrollY: scrollY, width: width)
    }

    /// Per-block top + bottom Y in scroll-content coordinates. One pass.
    private func computeBlockRanges(blocks: [Block], gap: CGFloat) -> [BlockRange] {
        var out: [BlockRange] = []
        out.reserveCapacity(blocks.count)
        var yPt: CGFloat = 0
        for block in blocks {
            let top = yPt
            let bot = top + headerHeightPt + bodyHeightPt(for: block)
            out.append(BlockRange(block: block, top: top, bot: bot))
            yPt = bot + gap
        }
        return out
    }

    private func placeDividerForRange(
        _ range: BlockRange, idx: Int, width: CGFloat,
        gap: CGFloat, color: UIColor
    ) {
        let inset = BlockPanelStyle.dividerHorizontalInsetPt
        let thickness = BlockPanelStyle.dividerThicknessPt
        let rect = CGRect(
            x: inset,
            y: range.top - gap / 2 - thickness / 2,
            width: max(0, width - inset * 2),
            height: thickness)
        placeDivider(for: range.block, isFirst: idx == 0, rect: rect, color: color)
    }

    /// (block, naturalTop, naturalBot) in scroll-content coordinates.
    /// Shared between header mounting and sticky-pinning.
    struct BlockRange {
        let block: Block
        let top: CGFloat
        let bot: CGFloat
    }

    private func pushLayoutToMetalView() {
        // Reentrancy guard: scrollToBottom() triggers
        // scrollViewDidScroll → pushLayoutToMetalView.
        guard !isPushingLayout else { return }
        isPushingLayout = true
        defer { isPushingLayout = false }

        let blocks = session.blockStore.blocks
        let scale = view.window?.screen.scale ?? 3.0
        let gap = BlockPanelStyle.interBlockGapPt
        let metalViewWidth = metalView.bounds.width
        var yPt: CGFloat = 0
        var entries: [BtBlockLayoutEntry] = []
        entries.reserveCapacity(blocks.count)
        for block in blocks {
            let bodyTop = yPt + headerHeightPt
            let bodyPt = bodyHeightPt(for: block)
            // We dropped the rounded panel chrome — pass `panel_bg_rgba=0`
            // and `panel_corner_radius_px=0` so Rust skips the panel draw
            // entirely. The hairline divider between blocks is painted on
            // the Swift side as a UIView in `syncHeaders`.
            entries.append(
                BtBlockLayoutEntry(
                    block_id: block.id,
                    body_y_top_px: Float(bodyTop * scale),
                    body_height_px: Float(bodyPt * scale),
                    panel_y_top_px: Float(yPt * scale),
                    panel_height_px: Float((headerHeightPt + bodyPt) * scale),
                    panel_x_left_px: 0,
                    panel_width_px: Float(metalViewWidth * scale),
                    panel_bg_rgba: 0,
                    panel_corner_radius_px: 0
                ))
            yPt += headerHeightPt + bodyPt + gap
        }
        metalView.update(scrollOffset: scrollView.contentOffset.y, layout: entries)
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
