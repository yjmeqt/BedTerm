import BedTermCoreC
import MetalKit
import UIKit

/// UIKit view that owns a `TerminalCore` + `RendererBridge` pair and drives
/// the Rust Metal renderer on every `setNeedsDisplay()` tick.
final class TerminalMetalUIView: MTKView {
    let terminalCore: TerminalCore
    let bridge: RendererBridge
    let onSend: (Data) -> Void
    private let onResize: (Int, Int) -> Void
    private weak var session: TerminalSession?
    /// When `true`, this view refuses first responder and drops all input
    /// (hardware keys, IME, soft-keyboard). Selection and scroll gestures
    /// still work so the user can copy from the read-only replay.
    let isInputDisabled: Bool

    private var startTime = CACurrentMediaTime()
    private var consumeTask: Task<Void, Never>?
    var anchorTask: Task<Void, Never>?
    private var lastCols: Int = 0
    private var lastRows: Int = 0
    private(set) var cellSize = CGSize(width: 8, height: 16)
    private let cursorLayer = MetalCursorLayer()
    private let selectionLayer = MetalSelectionLayer()
    private let selectionGR = MetalSelectionGesture(target: nil, action: nil)
    private var currentCols: Int = 80

    // Scroll state. Source of truth lives in TerminalCore (Rust grid's
    // display_offset). These fields only hold transient gesture math —
    // internal access is needed by the +Scroll extension file.
    var panGR: UIPanGestureRecognizer?
    var dragAccumulator: CGFloat = 0  // sub-row remainder (points)
    var displayLink: CADisplayLink?
    var scrollPhysics: ScrollPhysics?

    // MARK: Display mode

    /// Which rendering path `draw(_:)` takes. Set by the SwiftUI host via
    /// `TerminalMetalHostView.updateUIView`. The didSet gates gestures,
    /// cursor visibility, and first-responder status per mode.
    var displayMode: TerminalDisplayMode = .inline {
        didSet {
            let isBlock = displayMode == .blockList
            panGR?.isEnabled = !isBlock
            selectionGR.isEnabled = !isBlock
            isUserInteractionEnabled = !isBlock
            cursorLayer.isHidden = isBlock
            setNeedsDisplay()
        }
    }

    // MARK: Block-list layout (pushed by BlockListContainerViewController)

    private var blockScrollOffsetPx: CGFloat = 0
    private var blockEntries: [BtBlockLayoutEntry] = []
    private var blockHeaders: [BtBlockHeaderEntry] = []
    /// Contiguous UTF-8 blob that keeps `blockHeaders[*].command_utf8` /
    /// `subtitle_utf8` pointers alive across the FFI call in `draw(_:)`.
    private var blockHeaderBlob: [UInt8] = []

    /// Called by the block-list interaction controller before each redraw.
    /// `headers` must have their UTF-8 pointers already patched into
    /// `headerBlob` — the blob is retained here so pointers stay live
    /// through `draw(_:)`.
    func updateBlockLayout(
        scrollOffsetPx: CGFloat,
        entries: [BtBlockLayoutEntry],
        headers: [BtBlockHeaderEntry],
        headerBlob: [UInt8]
    ) {
        blockScrollOffsetPx = scrollOffsetPx
        blockEntries = entries
        blockHeaders = headers
        blockHeaderBlob = headerBlob
    }

    init(
        session: TerminalSession,
        feed: AsyncStream<Data>,
        onSend: @escaping (Data) -> Void,
        onResize: @escaping (Int, Int) -> Void
    ) {
        // Shared Metal context across the terminal pane and every Block view —
        // one atlas + pipeline state for the whole app. See MetalEnvironment.
        let env = MetalEnvironment.shared
        // Borrow the session-owned core; the session keeps it alive across
        // view tear-downs so Back→re-open restores the existing grid
        // (background-sessions P1: TerminalCore strong-ownership).
        self.terminalCore = session.terminalCore
        self.bridge = env.renderer
        self.onSend = onSend
        self.onResize = onResize
        self.session = session
        self.isInputDisabled = false
        super.init(frame: .zero, device: env.device)

        // Snap-on-input (R5.scroll_snap_on_input): one session hook covers
        // every PTY-bound byte. Core captured directly; weak self for
        // inertia-stop safety across dealloc.
        let core = self.terminalCore
        session.onBeforeSend = { [weak self] in
            guard core.scrollOffset > 0 else { return }
            core.scrollToBottom()
            self?.stopInertia()
            self?.setNeedsDisplay()
        }

        // framebufferOnly=false: we hand the drawable's texture across FFI as a
        // raw pointer, so the GPU pipeline must allow CPU-readable access.
        self.framebufferOnly = false
        self.colorPixelFormat = .bgra8Unorm
        // isPaused + enableSetNeedsDisplay: battery-friendly, redraw only on
        // explicit setNeedsDisplay() calls.
        self.isPaused = true
        self.enableSetNeedsDisplay = true
        // presentsWithTransaction: needed in Tasks 11/12 to keep cursor/
        // selection CALayers flicker-free; setting now avoids a later flip.
        self.presentsWithTransaction = true

        layer.addSublayer(cursorLayer)
        layer.addSublayer(selectionLayer)
        installGestureRecognizers()
        refreshFontMetrics()
        applyAppearance()
        installTraitObservers()

        // The session pump is authoritative for feeding the core, mode,
        // and block-store — it runs whether or not this view is mounted.
        // The view's loop is purely a redraw + screen-clear-anchor signal.
        consumeTask = Task { @MainActor [weak self] in
            for await chunk in feed {
                guard let self else { return }
                self.setNeedsDisplay()
                if Self.containsScreenClear(chunk) {
                    self.scheduleBottomAnchorPass()
                }
            }
        }
    }

    /// Read-only replay init: mounts an externally-owned `TerminalCore`
    /// (produced by `PersistenceHandle.openReplay`) without attaching a
    /// live session feed. Input is disabled; scroll and selection work.
    init(replayCore: TerminalCore) {
        let env = MetalEnvironment.shared
        self.terminalCore = replayCore
        self.bridge = env.renderer
        self.onSend = { _ in }
        self.onResize = { _, _ in }
        self.session = nil
        self.isInputDisabled = true
        super.init(frame: .zero, device: env.device)

        self.framebufferOnly = false
        self.colorPixelFormat = .bgra8Unorm
        self.isPaused = true
        self.enableSetNeedsDisplay = true
        self.presentsWithTransaction = true

        layer.addSublayer(cursorLayer)
        layer.addSublayer(selectionLayer)
        installGestureRecognizers()
        refreshFontMetrics()
        applyAppearance()
        installTraitObservers()
        // No consumeTask — no live feed; render once on appear.
        setNeedsDisplay()
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("not used") }

    /// Register iOS 17+ trait observers for Dynamic Type and Light/Dark
    /// appearance. Uses the closure form so `self` is the first argument,
    /// avoiding a retain cycle.
    private func installTraitObservers() {
        // Dynamic Type: re-rasterise atlas + recompute cell size when the
        // user's preferred content size category changes (iOS 17+ API).
        registerForTraitChanges(
            [UITraitPreferredContentSizeCategory.self]
        ) { (self: TerminalMetalUIView, _: UITraitCollection) in
            self.refreshFontMetrics()
            self.setNeedsLayout()
            self.setNeedsDisplay()
        }

        registerForTraitChanges(
            [UITraitUserInterfaceStyle.self]
        ) { (self: TerminalMetalUIView, prev: UITraitCollection) in
            guard prev.userInterfaceStyle != self.traitCollection.userInterfaceStyle else {
                return
            }
            self.applyAppearance()
        }
    }

    /// Resolve the current `TerminalPalette` for the active trait collection,
    /// push it to the Rust core, sync the renderer's clear colour, sync the
    /// MTKView's `clearColor` + UIView `backgroundColor`, and request a
    /// redraw. Called once at init and again from the
    /// `UITraitUserInterfaceStyle` trait observer.
    private func applyAppearance() {
        let palette = TerminalPalette.resolve(for: traitCollection)
        terminalCore.setPalette(palette)

        let bgR = Float(palette.defaultBg.r) / 255.0
        let bgG = Float(palette.defaultBg.g) / 255.0
        let bgB = Float(palette.defaultBg.b) / 255.0
        bridge.setClearColor(red: bgR, green: bgG, blue: bgB, alpha: 1.0)
        clearColor = MTLClearColor(
            red: Double(bgR),
            green: Double(bgG),
            blue: Double(bgB),
            alpha: 1.0
        )
        backgroundColor = UIColor(
            red: CGFloat(bgR),
            green: CGFloat(bgG),
            blue: CGFloat(bgB),
            alpha: 1.0
        )
        setNeedsDisplay()
    }

    private func installGestureRecognizers() {
        selectionGR.cellSize = cellSize
        selectionGR.onSelectionChange = { [weak self] sel in
            guard let self else { return }
            self.selectionLayer.update(sel, cellSize: self.cellSize, cols: self.currentCols)
        }
        selectionGR.onCopy = { [weak self] sel in
            guard let self else { return }
            UIPasteboard.general.string = self.extractSelectedText(sel)
        }
        addGestureRecognizer(selectionGR)

        // Single-tap brings up the system keyboard by making this view first
        // responder, and also stops any in-flight scroll inertia.
        // The long-press selection recogniser fires later (0.4 s
        // minimumPressDuration) so the two don't conflict.
        let focusTap = UITapGestureRecognizer(target: self, action: #selector(handleFocusTap))
        focusTap.cancelsTouchesInView = false
        addGestureRecognizer(focusTap)

        // One-finger pan scrolls the terminal viewport into scrollback.
        // Selection (long-press-then-drag) wins naturally because pan starts
        // firing as soon as the finger moves, while selection waits 0.4 s.
        let pan = UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:)))
        pan.maximumNumberOfTouches = 1
        pan.delegate = self
        addGestureRecognizer(pan)
        self.panGR = pan
    }

    isolated deinit {
        consumeTask?.cancel()
        anchorTask?.cancel()
        displayLink?.invalidate()
    }

    override func draw(_ rect: CGRect) {
        // Skip the GPU render pass when hidden behind the block-list overlay.
        guard let drawable = currentDrawable, window != nil, !isHidden, alpha > 0 else {
            return
        }
        let size = drawableSize
        bridge.setClearColor(
            red: Float(clearColor.red), green: Float(clearColor.green),
            blue: Float(clearColor.blue), alpha: Float(clearColor.alpha))

        switch displayMode {
        case .blockList:
            // blockHeaderBlob keeps header UTF-8 pointers alive.
            _ = blockHeaderBlob
            _ = bridge.drawBlockList(
                term: terminalCore,
                into: drawable.texture,
                viewport: size,
                scrollOffsetPx: blockScrollOffsetPx,
                layout: blockEntries,
                headers: blockHeaders
            )

        case .inline, .altScreen:
            let elapsed = CACurrentMediaTime() - startTime
            _ = bridge.draw(
                term: terminalCore,
                into: drawable.texture,
                viewport: size,
                time: elapsed
            )
            let snapshot = terminalCore.snapshot()
            let cursorRow = Int(snapshot.cursorRow)
            let cursorVisible = cursorRow < Int(snapshot.rows)
            cursorLayer.isHidden = !cursorVisible
            if cursorVisible {
                cursorLayer.update(
                    col: Int(snapshot.cursorCol),
                    row: cursorRow,
                    cellSize: cellSize
                )
            }
            updatePreeditOverlay()
        }

        let fence = bridge.queue.makeCommandBuffer()
        fence?.commit()
        fence?.waitUntilScheduled()
        drawable.present()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        // `cellSize` from UIKit text metrics is in POINTS. Use `bounds` (also
        // points) for col/row math. `drawableSize` is in pixels and only
        // belongs on the Metal-renderer side of the FFI.
        let pointSize = bounds.size
        guard cellSize.width > 0, cellSize.height > 0,
            pointSize.width > 0, pointSize.height > 0
        else { return }
        let cols = max(1, Int(pointSize.width / cellSize.width))
        let rows = max(1, Int(pointSize.height / cellSize.height))
        if cols != lastCols || rows != lastRows {
            let didGrow = rows > lastRows
            terminalCore.resize(cols: cols, rows: rows)
            onResize(cols, rows)
            lastCols = cols
            lastRows = rows
            setNeedsDisplay()
            // Re-anchor the prompt on every viewport growth so the shell's
            // initial banner + prompt land at the bottom of the new viewport
            // instead of the top. The anchor pass is a no-op when a TUI app
            // has drawn below the cursor.
            if didGrow, rows > 3, !isInputDisabled {
                scheduleBottomAnchorPass()
            }
        }
        currentCols = cols
        selectionGR.cellSize = cellSize
    }

    private func extractSelectedText(_ sel: SelectionRange) -> String {
        let snapshot = terminalCore.snapshot()
        let norm = sel.normalised
        let rowsCount = Int(snapshot.rows)
        let colsCount = Int(snapshot.cols)
        var out = ""
        let lastRow = min(norm.endRow, rowsCount - 1)
        guard norm.startRow <= lastRow else { return "" }
        for row in norm.startRow...lastRow {
            let from = (row == norm.startRow) ? norm.startCol : 0
            let to = (row == norm.endRow) ? norm.endCol : colsCount
            for col in from..<min(to, colsCount) {
                let scalar = snapshot.cell(col: col, row: row).flatMap { cell in
                    cell.ch != 0 ? Unicode.Scalar(cell.ch) : nil
                }
                out.append(scalar.map(Character.init) ?? " ")
            }
            if row != lastRow { out.append("\n") }
        }
        return out
    }

    @objc private func handleFocusTap() {
        // Tap during deceleration stops it immediately, even if the keyboard
        // is already showing — gives the user a brake on a flick.
        stopInertia()
        guard !isInputDisabled else { return }
        if !isFirstResponder {
            _ = becomeFirstResponder()
            reloadInputViews()
        }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        guard !isInputDisabled, displayMode != .blockList,
            window != nil, !isFirstResponder else { return }
        _ = becomeFirstResponder()
    }

    override var canBecomeFirstResponder: Bool { !isInputDisabled }

    private func refreshFontMetrics() {
        let body = UIFontMetrics.default.scaledFont(
            for: .monospacedSystemFont(ofSize: 14, weight: .regular)
        )
        let scale = traitCollection.displayScale > 0 ? traitCollection.displayScale : 2
        bridge.setFont(pointSize: body.pointSize, scale: scale)
        // Source the cell size from the renderer directly — anything else
        // (UIFont.lineHeight, NSString.size(withAttributes:)) introduces
        // ceiling-arithmetic drift between UIKit and CoreText that
        // accumulates per row and misaligns the cursor + selection
        // overlays from the rendered glyphs.
        cellSize = bridge.cellSizeInPoints(scale: scale)
    }

}

extension TerminalMetalUIView: UIGestureRecognizerDelegate {
    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        // Pan and selection are mutually exclusive: once selection starts
        // (long-press-then-drag), it owns the gesture; once pan starts (drag
        // immediately on touch), selection stays dormant. UIKit's natural
        // arbitration produces this without any require(toFail:) wiring.
        false
    }
}
