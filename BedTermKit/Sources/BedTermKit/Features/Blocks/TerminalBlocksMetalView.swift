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
    /// Header descriptors for the current frame. UTF-8 pointers inside
    /// each entry are mutated at FFI call time from `headerStorage` —
    /// see `update(scrollOffset:layout:headers:storage:)`.
    private var headers: [BtBlockHeaderEntry] = []
    /// Backing UTF-8 storage that keeps `headers[i].command_utf8` /
    /// `subtitle_utf8` pointers alive across the FFI call.
    private var headerStorage: [(command: Data, subtitle: Data?)] = []

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

    /// Pump fresh scroll offset + layout table + header descriptors;
    /// triggers redraw. `storage` keeps UTF-8 bytes alive while `headers`
    /// references them through raw pointers patched in at draw time.
    func update(
        scrollOffset: CGFloat,
        layout: [BtBlockLayoutEntry],
        headers: [BtBlockHeaderEntry],
        storage: [(command: Data, subtitle: Data?)]
    ) {
        self.scrollOffset = scrollOffset
        self.layout = layout
        self.headers = headers
        self.headerStorage = storage
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

        // Flatten every header's UTF-8 into a single contiguous buffer
        // and patch `command_utf8` / `subtitle_utf8` to point into it.
        // Pinning the entire blob through `withUnsafeBufferPointer`
        // gives a single, stable base address for the duration of the
        // FFI call — Swift's `Data.withUnsafeBytes` would only stabilise
        // each pointer per-closure, which is fragile for N strings.
        var blob: [UInt8] = []
        var commandRanges: [Range<Int>] = []
        commandRanges.reserveCapacity(headerStorage.count)
        var subtitleRanges: [Range<Int>?] = []
        subtitleRanges.reserveCapacity(headerStorage.count)
        for entry in headerStorage {
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
            var patched = headers
            let base = blobBuf.baseAddress
            for idx in patched.indices where idx < commandRanges.count {
                let cmdRange = commandRanges[idx]
                patched[idx].command_utf8 = base.map { $0.advanced(by: cmdRange.lowerBound) }
                if let subRange = subtitleRanges[idx] {
                    patched[idx].subtitle_utf8 = base.map { $0.advanced(by: subRange.lowerBound) }
                }
            }
            _ = env.renderer.drawBlockList(
                term: core,
                into: drawable.texture,
                viewport: size,
                scrollOffsetPx: scrollPx,
                layout: layout,
                headers: patched
            )
        }
        let fence = env.renderer.queue.makeCommandBuffer()
        fence?.commit()
        fence?.waitUntilScheduled()
        drawable.present()
    }
}
