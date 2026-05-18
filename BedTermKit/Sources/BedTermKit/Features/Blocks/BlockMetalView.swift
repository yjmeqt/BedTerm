import MetalKit
import SwiftUI
import UIKit

/// Hosts an `MTKView` that draws one block's body — either a frozen
/// `GridSnapshot` (sealed block) or a fresh row-range snapshot taken from
/// the live `TerminalCore` every frame (running block).
///
/// Each visible block instantiates one of these. `LazyVStack` virtualises
/// off-screen rows so the `MTKView` count stays bounded by what's on
/// screen. Each view creates its own `BtRenderer`; we share the
/// `MTLDevice` + `MTLCommandQueue` across views via a process-wide
/// singleton (see `MetalEnvironment`) so the atlas + queue aren't
/// duplicated per view.
struct BlockMetalView: UIViewRepresentable {
    /// What to draw. `frozen` means the snapshot is final; `live` means
    /// re-snapshot the range every redraw against the supplied core.
    enum Source {
        case frozen(GridSnapshot)
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

        let renderer = env.makeRenderer()
        context.coordinator.renderer = renderer
        context.coordinator.source = source
        view.delegate = context.coordinator
        view.setNeedsDisplay()
        return view
    }

    func updateUIView(_ view: MTKView, context: Context) {
        context.coordinator.source = source
        view.setNeedsDisplay()
    }

    static func dismantleUIView(_ view: MTKView, coordinator: Coordinator) {
        coordinator.renderer = nil
    }

    @MainActor
    final class Coordinator: NSObject, MTKViewDelegate {
        var renderer: RendererBridge?
        var source: Source?
        private var startTime = CACurrentMediaTime()

        func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
            view.setNeedsDisplay()
        }

        func draw(in view: MTKView) {
            guard let renderer, let source,
                let drawable = view.currentDrawable
            else { return }
            let size = view.drawableSize
            let elapsed = CACurrentMediaTime() - startTime
            let snapshot: GridSnapshot? = {
                switch source {
                case .frozen(let snap):
                    return snap
                case .live(let core, let startLine):
                    let end = core.currentLine + 1
                    guard end > startLine else { return nil }
                    return core.snapshotRange(startLine: startLine, endLine: end)
                }
            }()
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

/// Process-wide Metal device + command queue shared across the main
/// terminal view and every Block view. Reusing them keeps the GPU object
/// count bounded as scrollback grows. `makeRenderer()` mints a fresh
/// `BtRenderer` per view (atlas isn't shared yet — that's a follow-up
/// optimisation if memory becomes a concern).
@MainActor
final class MetalEnvironment {
    static let shared = MetalEnvironment()

    let device: MTLDevice
    let queue: MTLCommandQueue

    private init() {
        guard let device = MTLCreateSystemDefaultDevice(),
            let queue = device.makeCommandQueue()
        else {
            preconditionFailure("Metal initialisation failed")
        }
        self.device = device
        self.queue = queue
    }

    func makeRenderer() -> RendererBridge {
        guard let renderer = RendererBridge(device: device, queue: queue) else {
            preconditionFailure("Could not construct BtRenderer")
        }
        return renderer
    }
}
