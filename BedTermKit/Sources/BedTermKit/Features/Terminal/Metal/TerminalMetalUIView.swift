import MetalKit
import UIKit

/// UIKit view that owns a `TerminalCore` + `RendererBridge` pair and drives
/// the Rust Metal renderer on every `setNeedsDisplay()` tick. Plan B1 Task 9.
final class TerminalMetalUIView: MTKView {
    let terminalCore: TerminalCore
    let bridge: RendererBridge
    private let onSend: (Data) -> Void
    private let onResize: (Int, Int) -> Void

    private var startTime = CACurrentMediaTime()
    private var consumeTask: Task<Void, Never>?
    private var lastCols: Int = 0
    private var lastRows: Int = 0
    private(set) var cellSize = CGSize(width: 8, height: 16)

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

        refreshFontMetrics()

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
    }

    private func refreshFontMetrics() {
        let body = UIFontMetrics.default.scaledFont(
            for: .monospacedSystemFont(ofSize: 14, weight: .regular)
        )
        bridge.setFont(pointSize: body.pointSize, scale: UIScreen.main.scale)
        let charSize = ("M" as NSString).size(withAttributes: [.font: body])
        cellSize = charSize
    }
}
