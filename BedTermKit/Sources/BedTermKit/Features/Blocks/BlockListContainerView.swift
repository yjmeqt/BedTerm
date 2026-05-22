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
///   - `metalView` pinned sibling **beneath** the scrollView, paints all
///     visible block bodies via one `bt_renderer_draw_block_list` call
///     per frame. Lives below the scroll view so the SwiftUI header
///     hosts (inside `contentView`) render over its transparent regions
///     — otherwise the Metal layer's opaque body cells were the only
///     thing visible and every natural header got hidden, leaving only
///     the floating pinned header at the top of the screen.
@MainActor
final class BlockListContainerViewController: UIViewController, UIScrollViewDelegate {
    let session: TerminalSession
    let scrollView = UIScrollView()
    let contentView = UIView()
    private let metalView: TerminalBlocksMetalView

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
        view.addSubview(metalView)
        view.addSubview(scrollView)
        scrollView.addSubview(contentView)
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
    /// SwiftUI hierarchy. Cannot rely on deinit — the link IS what would
    /// be keeping us alive.
    func teardown() {
        displayLink?.invalidate()
        displayLink = nil
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

    var scrollPosition: ScrollPosition = .followsBottom
    var lastBlockCount: Int = 0

    /// Called by the SwiftUI wrapper's `updateUIViewController` whenever
    /// observed state changes (BlockStore.blocks). Rebuilds header hosts,
    /// content size, and the Metal layout table; honours the recorded
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
        applyScrollPosition(animated: false)
        // syncHeaders depends on the up-to-date contentOffset so it
        // runs AFTER the pin adjustment, not before.
        syncHeaders()
        pushLayoutToMetalView()
        updateDisplayLink()
    }

    private func applyScrollPosition(animated: Bool) {
        let maxOffset = max(0, scrollView.contentSize.height - scrollView.bounds.height)
        guard scrollView.bounds.height > 0 else { return }
        // While the user is dragging or the scroll view is decelerating
        // from a flick, the gesture owns contentOffset. Calling
        // `setContentOffset` here yanks the viewport back to our anchor
        // and visibly fights the finger — symptom: "can't scroll while
        // claude is running because the display link re-pins every
        // frame". Defer until interaction ends.
        guard !scrollView.isTracking, !scrollView.isDecelerating else { return }
        switch scrollPosition {
        case .followsBottom:
            scrollView.setContentOffset(CGPoint(x: 0, y: maxOffset), animated: animated)
        case .fixedAt(let anchorY):
            // Clamp to the new max in case content shrank (a TUI cleared
            // its block and the contentSize fell beneath the user's
            // saved offset).
            let clamped = min(max(0, anchorY), maxOffset)
            // Avoid re-issuing the same offset every frame — that's a
            // no-op for UIScrollView but it still flushes pending
            // animations and causes minor jank in Instruments.
            if abs(scrollView.contentOffset.y - clamped) > 0.5 {
                scrollView.setContentOffset(CGPoint(x: 0, y: clamped), animated: animated)
            }
        }
    }

    func isAtBottomEdge() -> Bool {
        let maxOffset = max(0, scrollView.contentSize.height - scrollView.bounds.height)
        return scrollView.contentOffset.y >= maxOffset - pinTolerancePt
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
        updateContentSize()
        applyScrollPosition(animated: false)
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
        // Full-width metalView. We used to inset by `cellLeftInsetPt`,
        // which made the GPU drawable 12pt narrower than what the PTY
        // was sized from (Classic view computes cols from the unindented
        // bounds), so the rightmost 1-2 cells were clipped off the
        // edge — characters at col 45 of a 46-wide grid simply vanished.
        // Headers/dividers already carry the visual left gutter via
        // SwiftUI padding, so the metalView no longer needs to.
        metalView.frame = CGRect(
            x: 0, y: 0,
            width: bounds.width,
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
        // `bodyRows` is Rust's `BlockGrid::used_rows()` for running
        // blocks and `end_line - start_line` for sealed ones. Tracking
        // it directly (instead of falling back to `core.screenRows`
        // while running) keeps the block visually flush with the
        // composer / next block as the TUI draws — no pre-allocated
        // 39-row canvas, no jarring collapse at seal time.
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
        let width = view.bounds.width
        scrollView.contentSize = CGSize(width: width, height: total)
        contentView.frame = CGRect(origin: .zero, size: scrollView.contentSize)
    }

    /// Update the (still-SwiftUI) sticky pin host for the currently
    /// pinned block. M5 will replace this with a renderer-drawn sticky
    /// descriptor; for now M4 keeps the overlay so the visual change
    /// is scoped to per-block headers only.
    func syncHeaders() {
        let blocks = session.blockStore.blocks
        let width = view.bounds.width
        let scrollY = scrollView.contentOffset.y
        let gap = BlockPanelStyle.interBlockGapPt
        let ranges = computeBlockRanges(blocks: blocks, gap: gap)
        updateStickyHeader(ranges: ranges, scrollY: scrollY, width: width)
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

    /// (block, naturalTop, naturalBot) in scroll-content coordinates.
    /// Shared between header mounting and sticky-pinning.
    struct BlockRange {
        let block: Block
        let top: CGFloat
        let bot: CGFloat
    }

    func pushLayoutToMetalView() {
        // Reentrancy guard: scrollToBottom() triggers
        // scrollViewDidScroll → pushLayoutToMetalView.
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
        let (headers, storage) = buildHeaderDescriptors(
            ranges: ranges, scale: Float(scale), widthPx: widthPx)
        metalView.update(
            scrollOffset: scrollView.contentOffset.y,
            layout: entries,
            headers: headers,
            storage: storage)
    }

    /// Resolve UI font pixel sizes from the current trait collection and
    /// push them to the Rust renderer. Called on viewDidLoad,
    /// traitCollectionDidChange, and viewWillTransition.
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
