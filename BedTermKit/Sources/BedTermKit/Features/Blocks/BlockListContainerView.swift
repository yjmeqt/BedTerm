import BedTermCoreC
import QuartzCore
import UIKit

/// Single-Metal-pass block list. The renderer paints bodies, header
/// bands, dividers, and the pinned sticky band in one
/// `bt_renderer_draw_block_list` call. The Swift container owns:
///
///   - `metalView`: full-frame MTKView that draws everything.
///   - `contentView`: transparent overlay whose frame.origin.y tracks
///     `-contentOffsetY`. Hosts `selectionLayer` so long-press points
///     resolve to content-space without per-frame translation.
///   - `panGR` + own `MomentumState`: replaces the old UIScrollView.
///     Mirrors Warp's `MomentumScroll` exactly (decay 0.968 per 8 ms).
@MainActor
final class BlockListContainerViewController: UIViewController {
    let session: TerminalSession
    /// Transparent overlay that scrolls in lockstep with contentOffsetY.
    /// Hosts the selection highlight layer; nothing visible draws here.
    let contentView = UIView()
    private let metalView: TerminalBlocksMetalView

    // MARK: Scroll state

    /// Current scroll position in content-pixel coordinates.
    var contentOffsetY: CGFloat = 0
    var contentHeight: CGFloat = 0
    var maxOffsetY: CGFloat { max(0, contentHeight - view.bounds.height) }

    let panGR = UIPanGestureRecognizer()
    var panStartOffsetY: CGFloat = 0
    var velocityEstimator = VelocityEstimator()
    var momentum: MomentumState?

    // MARK: Layout constants

    let headerHeightPt: CGFloat = 56
    let pinTolerancePt: CGFloat = 24

    // Atlas-derived metrics; refreshed from the shared renderer each
    // layout pass. Fallbacks cover the early-launch race where atlas
    // metrics aren't yet available.
    private var rowHeightPt: CGFloat = 18
    private var cellWidthPt: CGFloat = 9

    private var displayLink: CADisplayLink?
    private var isPushingLayout = false
    private var lastLayoutBounds: CGSize = .zero

    var scrollPosition: ScrollPosition = .followsBottom
    var lastBlockCount: Int = 0

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
        view.addSubview(metalView)
        view.addSubview(contentView)
        contentView.backgroundColor = .clear
        contentView.isUserInteractionEnabled = false

        panGR.delegate = self
        panGR.addTarget(self, action: #selector(handlePan(_:)))
        panGR.cancelsTouchesInView = false
        view.addGestureRecognizer(panGR)

        selection = BlockListSelectionController(
            contentView: contentView,
            headerHeightPt: headerHeightPt,
            hitResolver: { [weak self] point in self?.blockHitTest(at: point) },
            textResolver: { [weak self] id, range in
                self?.extractText(blockID: id, range: range)
            })
        view.addGestureRecognizer(selection.longPressGR)
        pushUIFontSizes()
    }

    /// Called explicitly by the SwiftUI representable's
    /// `dismantleUIViewController` so the CADisplayLink retain cycle
    /// (link → self) is broken before the controller drops out of the
    /// SwiftUI hierarchy. Cannot rely on deinit — the link IS what
    /// would be keeping us alive.
    func teardown() {
        displayLink?.invalidate()
        displayLink = nil
    }

    /// Called by the SwiftUI wrapper's `updateUIViewController` whenever
    /// observed state changes (BlockStore.blocks). Rebuilds content
    /// extents and the Metal layout table; honours the recorded
    /// `scrollPosition` so growing content doesn't yank a scrolled-back
    /// user.
    func refresh() {
        refreshRowHeight()
        // Mirror Warp's `AfterCommandExecutionStarted` — when a new
        // block lands, re-anchor to the bottom so the user always sees
        // the command they just submitted, even if they had previously
        // scrolled into history.
        let blockCount = session.blockStore.blocks.count
        if blockCount > lastBlockCount {
            scrollPosition = .followsBottom
        }
        lastBlockCount = blockCount
        updateContentSize()
        applyScrollPosition()
        pushLayoutToMetalView()
        updateDisplayLink()
    }

    // MARK: Display link

    func updateDisplayLink() {
        let hasRunning = session.blockStore.blocks.contains(where: \.isRunning)
        let needs = hasRunning || momentum != nil
        if needs {
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
        if momentum != nil { advanceMomentum(now: CACurrentMediaTime()) }
        updateContentSize()
        applyScrollPosition()
        pushLayoutToMetalView()
        if momentum == nil {
            updateDisplayLink()  // shed the link when both reasons go away
        }
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
        metalView.frame = bounds
        // contentView's height tracks the full content extent so the
        // selection layer can position itself in content-space.
        contentView.frame = CGRect(
            x: 0, y: -contentOffsetY,
            width: bounds.width, height: contentHeight)
        if bounds.size != lastLayoutBounds {
            lastLayoutBounds = bounds.size
            refresh()
        }
    }

    func bodyHeightPt(for block: Block) -> CGFloat {
        let rows = max(1, Int(block.bodyRows))
        return CGFloat(rows) * rowHeightPt
    }

    private func updateContentSize() {
        let blocks = session.blockStore.blocks
        let gap = BlockPanelStyle.interBlockGapPt
        var total: CGFloat = 0
        for block in blocks {
            total += headerHeightPt + bodyHeightPt(for: block) + gap
        }
        contentHeight = total
        // Re-frame contentView in case the height grew/shrank.
        contentView.frame = CGRect(
            x: 0, y: -contentOffsetY,
            width: view.bounds.width, height: contentHeight)
    }

    /// (block, naturalTop, naturalBot) in scroll-content coordinates.
    /// Shared between header descriptor building and sticky-pinning.
    struct BlockRange {
        let block: Block
        let top: CGFloat
        let bot: CGFloat
    }

    /// Per-block top + bottom Y in scroll-content coordinates. One pass.
    func computeBlockRanges(blocks: [Block], gap: CGFloat) -> [BlockRange] {
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

    func pushLayoutToMetalView() {
        guard !isPushingLayout else { return }
        isPushingLayout = true
        defer { isPushingLayout = false }

        let blocks = session.blockStore.blocks
        let scale = view.window?.screen.scale ?? 3.0
        let gap = BlockPanelStyle.interBlockGapPt
        let metalViewWidth = metalView.bounds.width
        let widthPx = Float(metalViewWidth * scale)
        let ranges = computeBlockRanges(blocks: blocks, gap: gap)

        var entries: [BtBlockLayoutEntry] = []
        entries.reserveCapacity(blocks.count)
        for range in ranges {
            let bodyTop = range.top + headerHeightPt
            let bodyPt = range.bot - bodyTop
            entries.append(
                BtBlockLayoutEntry(
                    block_id: range.block.id,
                    body_y_top_px: Float(bodyTop * scale),
                    body_height_px: Float(bodyPt * scale),
                    panel_y_top_px: Float(range.top * scale),
                    panel_height_px: Float((range.bot - range.top) * scale),
                    panel_x_left_px: 0,
                    panel_width_px: widthPx,
                    panel_bg_rgba: 0,
                    panel_corner_radius_px: 0
                ))
        }
        var (headers, storage) = buildHeaderDescriptors(
            ranges: ranges, scale: Float(scale), widthPx: widthPx)

        if let sticky = buildStickyDescriptor(
            ranges: ranges, scrollY: contentOffsetY,
            scale: Float(scale), widthPx: widthPx
        ) {
            if let idx = headers.firstIndex(where: { $0.block_id == sticky.blockID }) {
                headers.remove(at: idx)
                storage.remove(at: idx)
            }
            headers.append(sticky.entry)
            storage.append(sticky.storage)
        }

        metalView.update(
            scrollOffset: contentOffsetY,
            layout: entries,
            headers: headers,
            storage: storage)
    }

    /// Resolve UI font pixel sizes from the current trait collection
    /// and push them to the Rust renderer. Called on viewDidLoad and
    /// traitCollectionDidChange.
    func pushUIFontSizes() {
        let scale = view.window?.screen.scale ?? UIScreen.main.scale
        let traits = view.traitCollection
        let sub = UIFont.preferredFont(forTextStyle: .subheadline, compatibleWith: traits).pointSize
        let cap = UIFont.preferredFont(forTextStyle: .caption2, compatibleWith: traits).pointSize
        MetalEnvironment.shared.renderer.setUIFontSizes(
            subheadlinePx: Float(sub * scale),
            caption2Px: Float(cap * scale),
            scale: Float(scale))
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        pushUIFontSizes()
    }
}
