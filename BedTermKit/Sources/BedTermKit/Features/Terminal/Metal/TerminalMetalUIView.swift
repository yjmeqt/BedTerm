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
    private var lastCols: Int = 0
    private var lastRows: Int = 0
    private(set) var cellSize = CGSize(width: 8, height: 16)
    private let cursorLayer = MetalCursorLayer()
    private let selectionLayer = MetalSelectionLayer()
    private let selectionGR = MetalSelectionGesture(target: nil, action: nil)
    private var currentCols: Int = 80

    init(feed: AsyncStream<Data>,
         onSend: @escaping (Data) -> Void,
         onResize: @escaping (Int, Int) -> Void) {
        guard let device = MTLCreateSystemDefaultDevice(),
              let queue = device.makeCommandQueue(),
              let bridge = RendererBridge(device: device, queue: queue) else {
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
        selectionGR.cellSize = cellSize
        selectionGR.onSelectionChange = { [weak self] sel in
            guard let self else { return }
            self.selectionLayer.update(sel, cellSize: self.cellSize, cols: self.currentCols)
        }
        selectionGR.onCopy = { [weak self] sel in
            guard let self else { return }
            let text = self.extractSelectedText(sel)
            UIPasteboard.general.string = text
        }
        addGestureRecognizer(selectionGR)

        // Single-tap brings up the system keyboard by making this view first
        // responder. The long-press selection recogniser fires later (0.4 s
        // minimumPressDuration) so the two don't conflict.
        let focusTap = UITapGestureRecognizer(target: self, action: #selector(handleFocusTap))
        focusTap.cancelsTouchesInView = false
        addGestureRecognizer(focusTap)

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
                self.terminalCore.feed(chunk)
                self.setNeedsDisplay()
            }
        }
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("not used") }

    deinit { consumeTask?.cancel() }

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
        // Second command buffer is just for presentation — bridge.draw already
        // committed the cell pass on its own buffer. See task spec note.
        guard let cmd = bridge.queue.makeCommandBuffer() else { return }
        cmd.present(drawable)
        cmd.commit()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let size = drawableSize
        guard cellSize.width > 0, cellSize.height > 0 else { return }
        let cols = max(1, Int(size.width  / cellSize.width))
        let rows = max(1, Int(size.height / cellSize.height))
        if cols != lastCols || rows != lastRows {
            terminalCore.resize(cols: cols, rows: rows)
            onResize(cols, rows)
            lastCols = cols
            lastRows = rows
            setNeedsDisplay()
        }
        currentCols = cols
        selectionGR.cellSize = cellSize
    }

    private func extractSelectedText(_ sel: SelectionRange) -> String {
        let snapshot = terminalCore.snapshot()
        let n = sel.normalised
        let rowsCount = Int(snapshot.rows)
        let colsCount = Int(snapshot.cols)
        var out = ""
        let lastRow = min(n.endRow, rowsCount - 1)
        guard n.startRow <= lastRow else { return "" }
        for r in n.startRow...lastRow {
            let from = (r == n.startRow) ? n.startCol : 0
            let to   = (r == n.endRow)   ? n.endCol   : colsCount
            for c in from..<min(to, colsCount) {
                if let cell = snapshot.cell(col: c, row: r), cell.ch != 0,
                   let scalar = Unicode.Scalar(cell.ch) {
                    out.append(Character(scalar))
                } else {
                    out.append(" ")
                }
            }
            if r != lastRow { out.append("\n") }
        }
        return out
    }

    @objc private func handleFocusTap() {
        if !isFirstResponder { _ = becomeFirstResponder() }
    }

    private func refreshFontMetrics() {
        let body = UIFontMetrics.default.scaledFont(
            for: .monospacedSystemFont(ofSize: 14, weight: .regular)
        )
        bridge.setFont(pointSize: body.pointSize, scale: UIScreen.main.scale)
        let charSize = ("M" as NSString).size(withAttributes: [.font: body])
        cellSize = charSize
    }

    // MARK: First responder + keyboard input

    override var canBecomeFirstResponder: Bool { true }

    /// Hardware-key handling for keys UIKeyInput cannot deliver: arrows,
    /// escape, function keys, and Ctrl-combinations. Each maps to the
    /// canonical xterm/VT byte sequence and is sent straight to the PTY.
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

    private static func encode(key: UIKey) -> Data? {
        let esc: UInt8 = 0x1B
        // Ctrl-letter: produce the 0x01..0x1A control byte for A..Z.
        if key.modifierFlags.contains(.control), key.characters.count == 1,
           let ascii = key.characters.uppercased().unicodeScalars.first?.value,
           ascii >= 0x40, ascii <= 0x5F {
            return Data([UInt8(ascii - 0x40)])
        }
        switch key.keyCode {
        case .keyboardUpArrow:    return Data([esc, 0x5B, 0x41])
        case .keyboardDownArrow:  return Data([esc, 0x5B, 0x42])
        case .keyboardRightArrow: return Data([esc, 0x5B, 0x43])
        case .keyboardLeftArrow:  return Data([esc, 0x5B, 0x44])
        case .keyboardHome:       return Data([esc, 0x5B, 0x48])
        case .keyboardEnd:        return Data([esc, 0x5B, 0x46])
        case .keyboardPageUp:     return Data([esc, 0x5B, 0x35, 0x7E])
        case .keyboardPageDown:   return Data([esc, 0x5B, 0x36, 0x7E])
        case .keyboardEscape:     return Data([esc])
        case .keyboardTab:        return Data([0x09])
        case .keyboardReturnOrEnter: return Data([0x0D])
        case .keyboardDeleteOrBackspace: return Data([0x7F])
        default: return nil
        }
    }
}

extension TerminalMetalUIView: UIKeyInput {
    /// `UIKeyInput` requires this property; we never echo locally — the
    /// remote shell handles all echo, so there's no "text" we own.
    var hasText: Bool { false }

    func insertText(_ text: String) {
        // Translate a soft-keyboard Return into CR; everything else is UTF-8.
        if text == "\n" {
            onSend(Data([0x0D]))
        } else {
            onSend(Data(text.utf8))
        }
    }

    func deleteBackward() {
        onSend(Data([0x7F]))  // DEL — xterm-256color expects 0x7F, not 0x08
    }
}
