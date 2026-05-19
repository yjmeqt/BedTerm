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

    /// Fixed SwiftUI header strip height. Matches the BlockHeader's
    /// natural height with current padding (subheadline + caption2 + 10pt
    /// vertical padding ≈ 56pt). v1 limitation: may clip at Dynamic Type
    /// XXL — acceptable per Phase B plan.
    private let headerHeightPt: CGFloat = 56

    /// Cell row height in points, resolved from the shared renderer's
    /// atlas metrics. Fallback to 18 if metrics unavailable.
    private var rowHeightPt: CGFloat = 18

    /// CADisplayLink driving 60Hz redraws while any block is running.
    /// Stopped when the block list is fully sealed (battery hygiene).
    private var displayLink: CADisplayLink?

    /// Pin-to-bottom tolerance: contentOffset within this many points of
    /// the max counts as "at bottom" for auto-follow.
    private let pinTolerancePt: CGFloat = 24

    /// Reentrancy guard: scrollViewDidScroll fires inside
    /// `scrollToBottom(animated:false)`, which would re-push layout
    /// twice per tick. Bracket the push during state updates.
    private var isPushingLayout = false

    /// Last bounds we ran a full refresh against. layoutSubviews fires
    /// repeatedly during scroll, rotation, keyboard; short-circuit when
    /// nothing relevant has changed.
    private var lastLayoutBounds: CGSize = .zero

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
        scrollView.alwaysBounceVertical = true
        scrollView.backgroundColor = .clear
        contentView.backgroundColor = .clear
        view.addSubview(scrollView)
        scrollView.addSubview(contentView)
        view.addSubview(metalView)
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
        rebuildHeaders()
        updateContentSize()
        if wasPinnedToBottom {
            scrollToBottom(animated: false)
        }
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
        rebuildHeaders()
        updateContentSize()
        if wasPinnedToBottom {
            scrollToBottom(animated: false)
        }
        pushLayoutToMetalView()
    }

    private func refreshRowHeight() {
        let scale = view.window?.screen.scale ?? 3.0
        let metrics = MetalEnvironment.shared.renderer.cellSizeInPoints(scale: scale)
        if metrics.height >= 8 && metrics.height <= 64 {
            rowHeightPt = metrics.height
        }
    }

    override func viewWillLayoutSubviews() {
        super.viewWillLayoutSubviews()
        let bounds = view.bounds
        scrollView.frame = bounds
        metalView.frame = bounds
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
        var total: CGFloat = 0
        for block in blocks {
            total += headerHeightPt
            total += bodyHeightPt(for: block)
        }
        let width = view.bounds.width
        scrollView.contentSize = CGSize(width: width, height: total)
        contentView.frame = CGRect(origin: .zero, size: scrollView.contentSize)
    }

    private func rebuildHeaders() {
        let blocks = session.blockStore.blocks
        let liveIDs = Set(blocks.map(\.id))
        // Drop hosts whose block is gone — proper child-VC teardown.
        for (id, host) in headerHosts where !liveIDs.contains(id) {
            host.willMove(toParent: nil)
            host.view.removeFromSuperview()
            host.removeFromParent()
            headerHosts.removeValue(forKey: id)
        }
        var yPt: CGFloat = 0
        let width = view.bounds.width
        for block in blocks {
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
            host.view.frame = CGRect(x: 0, y: yPt, width: width, height: headerHeightPt)
            yPt += headerHeightPt
            yPt += bodyHeightPt(for: block)
        }
    }

    private func pushLayoutToMetalView() {
        // Reentrancy guard: scrollToBottom() triggers
        // scrollViewDidScroll → pushLayoutToMetalView. Without the
        // guard we'd rebuild the layout array twice per tick.
        guard !isPushingLayout else { return }
        isPushingLayout = true
        defer { isPushingLayout = false }

        let blocks = session.blockStore.blocks
        let scale = view.window?.screen.scale ?? 3.0
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
