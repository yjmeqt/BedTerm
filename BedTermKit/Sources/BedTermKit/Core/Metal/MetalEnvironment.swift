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
        // Hand the host monospace face (SF Mono → Menlo) to the Rust
        // rasterizer. Must run after the renderer is built so the
        // FontSystem singleton exists; a failure here is silent — Rust
        // falls back to bundled JetBrains Mono.
        let picked = TerminalFontBootstrap.registerHostMonospace()
        if let picked {
            print("[BedTerm] terminal font: \(picked)")
        } else {
            print("[BedTerm] terminal font: bundled JetBrains Mono (host load failed)")
        }
    }
}
