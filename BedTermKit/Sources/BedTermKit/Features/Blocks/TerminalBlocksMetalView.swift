import BedTermCoreC
import MetalKit
import UIKit

/// Single full-viewport MTKView that paints every visible block body
/// in one frame via `bt_renderer_draw_block_list`. Sibling of the host
/// UIScrollView — it does NOT scroll itself. The host pumps
/// `scrollOffset` + the per-block layout table in via
/// `update(scrollOffset:layout:)` before each redraw.
///
/// `isUserInteractionEnabled = false` so taps fall through to the
/// UIScrollView (whose content view holds the SwiftUI block headers).
@MainActor
final class TerminalBlocksMetalView: MTKView {
    private weak var session: TerminalSession?
    private var scrollOffset: CGFloat = 0
    private var layout: [BtBlockLayoutEntry] = []

    init(session: TerminalSession) {
        self.session = session
        let env = MetalEnvironment.shared
        super.init(frame: .zero, device: env.device)
        framebufferOnly = false
        colorPixelFormat = .bgra8Unorm
        isPaused = true
        enableSetNeedsDisplay = true
        presentsWithTransaction = true
        backgroundColor = .clear
        layer.isOpaque = false
        clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
        isUserInteractionEnabled = false
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("not used") }

    /// Pump fresh scroll offset + layout table; triggers redraw.
    func update(scrollOffset: CGFloat, layout: [BtBlockLayoutEntry]) {
        self.scrollOffset = scrollOffset
        self.layout = layout
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect) {
        guard let drawable = currentDrawable, let session, let core = session.terminalCore
        else {
            return
        }
        let env = MetalEnvironment.shared
        // Block-list pane sits over a SwiftUI background — clear transparent.
        env.renderer.setClearColor(red: 0, green: 0, blue: 0, alpha: 0)
        let size = drawableSize
        let scrollPx = scrollOffset * contentScaleFactor
        _ = env.renderer.drawBlockList(
            term: core,
            into: drawable.texture,
            viewport: size,
            scrollOffsetPx: scrollPx,
            layout: layout
        )
        let fence = env.renderer.queue.makeCommandBuffer()
        fence?.commit()
        fence?.waitUntilScheduled()
        drawable.present()
    }
}
