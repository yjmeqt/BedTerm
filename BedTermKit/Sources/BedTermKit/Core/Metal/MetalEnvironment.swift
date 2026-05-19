import MetalKit

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
