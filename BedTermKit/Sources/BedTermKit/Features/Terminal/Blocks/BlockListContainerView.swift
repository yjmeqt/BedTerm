import BedTermCoreC
import QuartzCore
import UIKit

/// Block-list interaction controller. Pushes per-frame layout to the shared
/// `TerminalMetalUIView` (which renders bodies, headers, dividers, and the
/// sticky band in one Metal pass). This controller provides the transparent
/// gesture overlay — pan, selection, scroll physics — without owning its
/// own MTKView.
@MainActor
final class BlockListContainerViewController: UIViewController {
    let session: TerminalSession
    /// Transparent overlay that scrolls in lockstep with contentOffsetY.
    /// Hosts the selection highlight layer; nothing visible draws here.
    let contentView = UIView()
    private weak var sharedMetalView: TerminalMetalUIView?

    // MARK: Scroll state

    /// Current scroll position in content-pixel coordinates.
    var contentOffsetY: CGFloat = 0
    var contentHeight: CGFloat = 0
    var maxOffsetY: CGFloat { max(0, contentHeight - view.bounds.height) }

    let panGR = UIPanGestureRecognizer()
    var panStartOffsetY: CGFloat = 0
    var velocityEstimator = VelocityEstimator()
    var scrollPhysics: ScrollPhysics?

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
    var selectionController: BlockListSelectionController!

    init(session: TerminalSession, sharedMetalView: TerminalMetalUIView) {
        self.session = session
        self.sharedMetalView = sharedMetalView
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
        view.addSubview(contentView)
        contentView.backgroundColor = .clear
        contentView.isUserInteractionEnabled = false

        panGR.delegate = self
        panGR.addTarget(self, action: #selector(handlePan(_:)))
        panGR.cancelsTouchesInView = false
        view.addGestureRecognizer(panGR)

        selectionController = BlockListSelectionController(
            contentView: contentView,
            headerHeightPt: headerHeightPt,
            hitResolver: { [weak self] point in self?.blockHitTest(at: point) },
            textResolver: { [weak self] id, range in
                self?.extractText(blockID: id, range: range)
            },
            scrollOffsetProvider: { [weak self] in self?.contentOffsetY ?? 0 })
        view.addGestureRecognizer(selectionController.longPressGR)
        pushUIFontSizes()
        installTraitObservers()
    }

    /// Palette + font push on Light↔Dark flip. The shared MTKView's own
    /// trait observer pushes the palette via `applyAppearance()`, and
    /// `TerminalMetalUIView.displayMode.didSet` triggers a redraw — we
    /// just push UI font sizes and re-layout. Pushing the palette to
    /// `terminalCore` here ensures block cell colour resolution stays
    /// in sync even if this observer fires before the MTKView's.
    private func installTraitObservers() {
        registerForTraitChanges(
            [UITraitUserInterfaceStyle.self]
        ) { (self: BlockListContainerViewController, prev: UITraitCollection) in
            guard prev.userInterfaceStyle != self.traitCollection.userInterfaceStyle else {
                return
            }
            self.session.terminalCore.setPalette(
                TerminalPalette.resolve(for: self.view.traitCollection))
            self.pushUIFontSizes()
            self.pushLayoutToMetalView()
        }
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
        let needs = hasRunning || scrollPhysics != nil
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
        if scrollPhysics != nil { advanceMomentum(now: CACurrentMediaTime()) }
        updateContentSize()
        applyScrollPosition()
        pushLayoutToMetalView()
        if scrollPhysics == nil {
            updateDisplayLink()
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
        selectionController?.updateMetrics(
            cellWidth: cellWidthPt, rowHeight: rowHeightPt,
            containerWidth: view.bounds.width,
            leftInset: BlockPanelStyle.cellLeftInsetPt)
    }

    override func viewWillLayoutSubviews() {
        super.viewWillLayoutSubviews()
        let bounds = view.bounds
        contentView.frame = bounds
        if bounds.size != lastLayoutBounds {
            lastLayoutBounds = bounds.size
            refresh()
        }
    }

    func bodyHeightPt(for block: Block) -> CGFloat {
        let rows = Int(block.bodyRows)
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
        guard !isPushingLayout, let metalView = sharedMetalView else { return }
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

        let blob = Self.patchHeaderPointers(
            headers: &headers, storage: storage)

        let scrollOffsetPx = contentOffsetY * scale
        metalView.updateBlockLayout(
            scrollOffsetPx: scrollOffsetPx,
            entries: entries,
            headers: headers,
            headerBlob: blob)
        metalView.setNeedsDisplay()
    }

    /// Flatten header UTF-8 storage into a contiguous blob and patch
    /// `headers[*].command_utf8` / `subtitle_utf8` to point into it.
    /// Returns the blob; the caller must keep it alive while the headers
    /// are in use (it's handed to `TerminalMetalUIView.updateBlockLayout`).
    static func patchHeaderPointers(
        headers: inout [BtBlockHeaderEntry],
        storage: [(command: Data, subtitle: Data?)]
    ) -> [UInt8] {
        var blob: [UInt8] = []
        var commandRanges: [Range<Int>] = []
        commandRanges.reserveCapacity(storage.count)
        var subtitleRanges: [Range<Int>?] = []
        subtitleRanges.reserveCapacity(storage.count)
        for entry in storage {
            let cStart = blob.count
            blob.append(contentsOf: entry.command)
            commandRanges.append(cStart..<blob.count)
            if let sub = entry.subtitle, !sub.isEmpty {
                let sStart = blob.count
                blob.append(contentsOf: sub)
                subtitleRanges.append(sStart..<blob.count)
            } else {
                subtitleRanges.append(nil)
            }
        }
        blob.withUnsafeBufferPointer { blobBuf in
            let base = blobBuf.baseAddress
            for idx in headers.indices where idx < commandRanges.count {
                let cmdRange = commandRanges[idx]
                headers[idx].command_utf8 = base?.advanced(by: cmdRange.lowerBound)
                if let subRange = subtitleRanges[idx] {
                    headers[idx].subtitle_utf8 = base?.advanced(by: subRange.lowerBound)
                }
            }
        }
        return blob
    }

    /// Resolve UI font pixel sizes from the current trait collection
    /// and push them to the Rust renderer.
    func pushUIFontSizes() {
        let scale = view.window?.screen.scale ?? view.traitCollection.displayScale
        let traits = view.traitCollection
        let sub = UIFont.preferredFont(forTextStyle: .subheadline, compatibleWith: traits).pointSize
        let cap = UIFont.preferredFont(forTextStyle: .caption2, compatibleWith: traits).pointSize
        MetalEnvironment.shared.renderer.setUIFontSizes(
            subheadlinePx: Float(sub * scale),
            caption2Px: Float(cap * scale),
            scale: Float(scale))
    }

}
