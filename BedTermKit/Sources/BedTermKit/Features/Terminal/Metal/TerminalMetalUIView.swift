import MetalKit
import UIKit

/// UIKit view that owns a `TerminalCore` + `RendererBridge` pair and drives
/// the Rust Metal renderer on every `setNeedsDisplay()` tick. Plan B1 Task 9.
final class TerminalMetalUIView: MTKView {
    let terminalCore: TerminalCore
    let bridge: RendererBridge
    let onSend: (Data) -> Void
    private let onResize: (Int, Int) -> Void

    private var startTime = CACurrentMediaTime()
    private var consumeTask: Task<Void, Never>?
    private var anchorTask: Task<Void, Never>?
    private var lastCols: Int = 0
    private var lastRows: Int = 0
    private(set) var cellSize = CGSize(width: 8, height: 16)
    private let cursorLayer = MetalCursorLayer()
    private let selectionLayer = MetalSelectionLayer()
    private let selectionGR = MetalSelectionGesture(target: nil, action: nil)
    private var currentCols: Int = 80

    init(
        feed: AsyncStream<Data>,
        onSend: @escaping (Data) -> Void,
        onResize: @escaping (Int, Int) -> Void
    ) {
        guard let device = MTLCreateSystemDefaultDevice(),
            let queue = device.makeCommandQueue(),
            let bridge = RendererBridge(device: device, queue: queue)
        else {
            preconditionFailure("Metal initialisation failed")
        }
        self.terminalCore = TerminalCore(cols: 80, rows: 24)
        self.bridge = bridge
        self.onSend = onSend
        self.onResize = onResize
        super.init(frame: .zero, device: device)

        // framebufferOnly=false: we hand the drawable's texture across FFI as a
        // raw pointer, so the GPU pipeline must allow CPU-readable access.
        self.framebufferOnly = false
        self.colorPixelFormat = .bgra8Unorm
        self.clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        // isPaused + enableSetNeedsDisplay: battery-friendly, redraw only on
        // explicit setNeedsDisplay() calls.
        self.isPaused = true
        self.enableSetNeedsDisplay = true
        // presentsWithTransaction: needed in Tasks 11/12 to keep cursor/
        // selection CALayers flicker-free; setting now avoids a later flip.
        self.presentsWithTransaction = true
        self.backgroundColor = .black

        layer.addSublayer(cursorLayer)
        layer.addSublayer(selectionLayer)
        installGestureRecognizers()
        refreshFontMetrics()

        // Dynamic Type: re-rasterise atlas + recompute cell size when the
        // user's preferred content size category changes. Uses the iOS 17+
        // trait-observation API (the legacy traitCollectionDidChange override
        // is deprecated on iOS 26). The closure form takes `self` as its first
        // argument, which avoids creating a retain cycle.
        registerForTraitChanges(
            [UITraitPreferredContentSizeCategory.self]
        ) { (self: TerminalMetalUIView, _: UITraitCollection) in
            self.refreshFontMetrics()
            self.setNeedsLayout()
            self.setNeedsDisplay()
        }

        consumeTask = Task { @MainActor [weak self] in
            for await chunk in feed {
                guard let self else { return }
                let hadScreenClear = Self.containsScreenClear(chunk)
                self.terminalCore.feed(chunk)
                self.setNeedsDisplay()
                if hadScreenClear { self.scheduleBottomAnchorPass() }
            }
        }
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("not used") }

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
        // responder. The long-press selection recogniser fires later (0.4 s
        // minimumPressDuration) so the two don't conflict.
        let focusTap = UITapGestureRecognizer(target: self, action: #selector(handleFocusTap))
        focusTap.cancelsTouchesInView = false
        addGestureRecognizer(focusTap)
    }

    deinit {
        consumeTask?.cancel()
        anchorTask?.cancel()
    }

    override func draw(_ rect: CGRect) {
        guard let drawable = currentDrawable else { return }
        let size = drawableSize
        let elapsed = CACurrentMediaTime() - startTime
        _ = bridge.draw(
            term: terminalCore,
            into: drawable.texture,
            viewport: size,
            time: elapsed
        )
        let snapshot = terminalCore.snapshot()
        cursorLayer.update(
            col: Int(snapshot.cursorCol),
            row: Int(snapshot.cursorRow),
            cellSize: cellSize
        )
        updatePreeditOverlay()
        // presentsWithTransaction=true requires a synchronous present: wait
        // for the cell-pass command buffer to be scheduled, then present the
        // drawable in the current CATransaction. Using cmd.present(drawable)
        // here would queue an async present that lands one frame after the
        // cursor CALayer commits, causing the cells-lag-cursor symptom.
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
            // Match SwiftTerm path: re-anchor the prompt on every viewport
            // growth so the shell's initial banner + prompt land at the
            // bottom of the new viewport instead of the top. The anchor
            // pass is a no-op when a TUI app has drawn below the cursor.
            if didGrow, rows > 3 {
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
        if !isFirstResponder {
            _ = becomeFirstResponder()
            reloadInputViews()
        }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil, !isFirstResponder {
            _ = becomeFirstResponder()
        }
    }

    override var canBecomeFirstResponder: Bool { true }

    // MARK: Hardware-key handling

    /// Hardware keys UIKeyInput cannot deliver: arrows, escape, function,
    /// Ctrl-combos. Each maps to the canonical xterm/VT byte sequence.
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var handled = false
        for press in presses {
            guard let key = press.key else { continue }
            if let bytes = Self.encode(key: key) {
                onSend(bytes)
                handled = true
            }
        }
        if !handled { super.pressesBegan(presses, with: event) }
    }

    private static let esc: UInt8 = 0x1B

    /// Static map from UIKey.keyCode to the canonical xterm/VT byte sequence.
    private static let keyCodeBytes: [UIKeyboardHIDUsage: [UInt8]] = [
        .keyboardUpArrow: [esc, 0x5B, 0x41],
        .keyboardDownArrow: [esc, 0x5B, 0x42],
        .keyboardRightArrow: [esc, 0x5B, 0x43],
        .keyboardLeftArrow: [esc, 0x5B, 0x44],
        .keyboardHome: [esc, 0x5B, 0x48],
        .keyboardEnd: [esc, 0x5B, 0x46],
        .keyboardPageUp: [esc, 0x5B, 0x35, 0x7E],
        .keyboardPageDown: [esc, 0x5B, 0x36, 0x7E],
        .keyboardEscape: [esc],
        .keyboardTab: [0x09],
        .keyboardReturnOrEnter: [0x0D],
        .keyboardDeleteOrBackspace: [0x7F]
    ]

    private static func encode(key: UIKey) -> Data? {
        if let ctrlByte = encodeControl(key: key) { return Data([ctrlByte]) }
        return keyCodeBytes[key.keyCode].map { Data($0) }
    }

    private static func encodeControl(key: UIKey) -> UInt8? {
        guard key.modifierFlags.contains(.control), key.characters.count == 1,
            let ascii = key.characters.uppercased().unicodeScalars.first?.value,
            ascii >= 0x40, ascii <= 0x5F
        else { return nil }
        return UInt8(ascii - 0x40)
    }

    private func refreshFontMetrics() {
        let body = UIFontMetrics.default.scaledFont(
            for: .monospacedSystemFont(ofSize: 14, weight: .regular)
        )
        let scale = UIScreen.main.scale
        bridge.setFont(pointSize: body.pointSize, scale: scale)
        // Source the cell size from the renderer directly — anything else
        // (UIFont.lineHeight, NSString.size(withAttributes:)) introduces
        // ceiling-arithmetic drift between UIKit and CoreText that
        // accumulates per row and misaligns the cursor + selection
        // overlays from the rendered glyphs.
        cellSize = bridge.cellSizeInPoints(scale: scale)
    }

    // MARK: Bottom anchoring (parity with TerminalHostView.Coordinator)

    // Standard "clear screen" CSI sequences emitted by `clear`, Ctrl+L,
    // `tput clear`, `reset`, and the `ESC c` RIS code. We bottom-anchor
    // only after one of these + a quiet period, so TUI apps that clear
    // before drawing their own UI are not disrupted.
    private static let screenClearPatterns: [[UInt8]] = [
        Array("\u{1B}[2J".utf8),
        Array("\u{1B}[3J".utf8),
        [0x1B, 0x63]
    ]

    private static func containsScreenClear(_ chunk: Data) -> Bool {
        guard !chunk.isEmpty else { return false }
        let bytes = [UInt8](chunk)
        for pattern in screenClearPatterns where indexOfSubsequence(of: pattern, in: bytes) != nil {
            return true
        }
        return false
    }

    private static func indexOfSubsequence(of needle: [UInt8], in haystack: [UInt8]) -> Int? {
        guard !needle.isEmpty, haystack.count >= needle.count else { return nil }
        let last = haystack.count - needle.count
        for offset in 0...last {
            var match = true
            for pos in 0..<needle.count where haystack[offset + pos] != needle[pos] {
                match = false
                break
            }
            if match { return offset }
        }
        return nil
    }

    private func scheduleBottomAnchorPass() {
        anchorTask?.cancel()
        anchorTask = Task { @MainActor [weak self] in
            // Give the shell ~120 ms to finish emitting its new prompt before
            // we decide. TUI apps keep streaming during this window, which
            // keeps the post-flight emptiness check below failing and the
            // anchor pass a no-op.
            try? await Task.sleep(nanoseconds: 120_000_000)
            if Task.isCancelled { return }
            self?.applyBottomAnchorIfShellAtTop()
        }
    }

    private func applyBottomAnchorIfShellAtTop() {
        let snapshot = terminalCore.snapshot()
        let rows = Int(snapshot.rows)
        let cols = Int(snapshot.cols)
        let row = Int(snapshot.cursorRow)
        let col = Int(snapshot.cursorCol)
        guard rows > 3, row >= 0, row < rows / 2 else { return }
        // Only anchor when every visible row strictly below the cursor is blank.
        for rowIndex in (row + 1)..<rows {
            for col in 0..<cols {
                if let cell = snapshot.cell(col: col, row: rowIndex), cell.ch != 0 {
                    return
                }
            }
        }
        let linesToInsert = rows - 1 - row
        guard linesToInsert > 0 else { return }
        // Same CSI dance as the SwiftTerm path: move to home, insert N blank
        // lines (which pushes the existing prompt row down to the bottom),
        // then re-park the cursor on the same column of the new bottom row.
        let sequence = "\u{1B}[1;1H\u{1B}[\(linesToInsert)L\u{1B}[\(rows);\(col + 1)H"
        terminalCore.feed(Data(sequence.utf8))
        setNeedsDisplay()
    }
}
