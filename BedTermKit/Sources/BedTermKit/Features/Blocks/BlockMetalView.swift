import MetalKit
import SwiftUI
import UIKit

/// Hosts an `MTKView` that draws one block's body — either a frozen
/// `GridSnapshot` (sealed block) or a fresh row-range snapshot taken from
/// the live `TerminalCore` every frame (running block).
///
/// Every Metal-backed view in the app shares the same `RendererBridge`
/// (and therefore the same atlas + pipeline state + GPU resources) via
/// `MetalEnvironment.shared`. Each MTKView delegate sets the renderer's
/// clear colour immediately before its draw call — main-thread
/// sequencing makes that race-free.
struct BlockMetalView: UIViewRepresentable {
    /// What to draw. `frozen` means the snapshot is final; `live` means
    /// re-snapshot the range every redraw against the supplied core.
    enum Source {
        /// Sealed block: fetch the frozen snapshot from Rust by block id.
        case frozen(core: TerminalCore, blockID: UInt64)
        case live(core: TerminalCore, startLine: Int32)
    }

    let source: Source

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MTKView {
        let env = MetalEnvironment.shared
        let view = MTKView(frame: .zero, device: env.device)
        view.framebufferOnly = false
        view.colorPixelFormat = .bgra8Unorm
        view.isPaused = true
        view.enableSetNeedsDisplay = true
        view.presentsWithTransaction = true
        view.backgroundColor = .clear
        view.layer.isOpaque = false
        view.clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)

        context.coordinator.source = source
        view.delegate = context.coordinator
        view.setNeedsDisplay()
        return view
    }

    func updateUIView(_ view: MTKView, context: Context) {
        context.coordinator.source = source
        view.setNeedsDisplay()
    }

    @MainActor
    final class Coordinator: NSObject, MTKViewDelegate {
        var source: Source?
        private var startTime = CACurrentMediaTime()

        func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
            view.setNeedsDisplay()
        }

        func draw(in view: MTKView) {
            guard let source,
                let drawable = view.currentDrawable
            else { return }
            let renderer = MetalEnvironment.shared.renderer
            let size = view.drawableSize
            let elapsed = CACurrentMediaTime() - startTime
            let snapshot: GridSnapshot? = {
                switch source {
                case .frozen(let core, let blockID):
                    guard
                        let idx = core.allBlocks().firstIndex(where: {
                            $0.id == blockID
                        })
                    else { return nil }
                    return core.frozenSnapshot(forBlockAt: idx)
                case .live(let core, let startLine):
                    let end = core.currentLine + 1
                    guard end > startLine else { return nil }
                    return core.snapshotRange(startLine: startLine, endLine: end)
                }
            }()
            // Block bodies sit over a SwiftUI background; the renderer's
            // clear colour is transparent so SwiftUI's surface shows in any
            // gap. The terminal pane resets this to its palette bg right
            // before its own draw — main-thread sequencing makes the shared
            // state safe.
            renderer.setClearColor(red: 0, green: 0, blue: 0, alpha: 0)
            if let snapshot {
                renderer.drawCells(
                    snapshot, into: drawable.texture, viewport: size, time: elapsed)
            }
            let fence = renderer.queue.makeCommandBuffer()
            fence?.commit()
            fence?.waitUntilScheduled()
            drawable.present()
        }
    }
}

/// Process-wide Metal device, command queue, and renderer. One atlas +
/// pipeline state across the live terminal pane and every block-row
/// view. Per-call clear colour is set by each MTKView delegate
/// immediately before drawing.
@MainActor
final class MetalEnvironment {
    static let shared = MetalEnvironment()

    let device: MTLDevice
    let queue: MTLCommandQueue
    let renderer: RendererBridge

    private init() {
        guard let device = MTLCreateSystemDefaultDevice(),
            let queue = device.makeCommandQueue(),
            let renderer = RendererBridge(device: device, queue: queue)
        else {
            preconditionFailure("Metal initialisation failed")
        }
        self.device = device
        self.queue = queue
        self.renderer = renderer
    }
}
