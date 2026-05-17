import BedTermCoreC
import Metal

final class RendererBridge {
    private let handle: OpaquePointer
    let device: MTLDevice
    let queue: MTLCommandQueue

    init?(device: MTLDevice, queue: MTLCommandQueue) {
        // Ownership contract (see Rust `Renderer::from_ptrs` docs):
        // Rust takes +1 retain on both device and queue. We MUST use
        // `passRetained` so the retain count is balanced when Rust drops
        // the wrappers. `passUnretained` would underflow the retain count.
        let devPtr = Unmanaged.passRetained(device as AnyObject).toOpaque()
        let qPtr = Unmanaged.passRetained(queue as AnyObject).toOpaque()
        guard let ptr = bt_renderer_new(devPtr, qPtr) else {
            // bt_renderer_new returned null: it never took ownership, so we
            // must release our retains explicitly.
            Unmanaged<AnyObject>.fromOpaque(devPtr).release()
            Unmanaged<AnyObject>.fromOpaque(qPtr).release()
            return nil
        }
        self.handle = ptr
        self.device = device
        self.queue = queue
    }

    deinit {
        bt_renderer_free(handle)
    }

    func setFont(pointSize: CGFloat, scale: CGFloat) {
        bt_renderer_set_font(handle, Float(pointSize), Float(scale))
    }

    /// Authoritative cell size in points, derived from the renderer's atlas
    /// (CoreText `ascent + descent + leading` rasterised at `scale`).
    /// CALayer overlays (cursor, selection) must use this to stay aligned
    /// with the rendered glyphs — UIFont's text-size metrics can drift by
    /// fractions of a pt per row and accumulate visually over the viewport.
    func cellSizeInPoints(scale: CGFloat) -> CGSize {
        var width: UInt32 = 0
        var height: UInt32 = 0
        bt_renderer_cell_pixel_size(handle, &width, &height)
        guard width > 0, height > 0, scale > 0 else { return .zero }
        return CGSize(width: CGFloat(width) / scale, height: CGFloat(height) / scale)
    }

    /// Encode one draw of `term` into `texture`. Returns 0 on success,
    /// negative on FFI-side error.
    @discardableResult
    func draw(term: TerminalCore, into texture: MTLTexture, viewport: CGSize, time: CFTimeInterval) -> Int32 {
        // `texture` is borrowed for the call only — Rust uses ManuallyDrop
        // so it will NOT release. passUnretained is correct.
        let texPtr = Unmanaged.passUnretained(texture as AnyObject).toOpaque()
        return bt_renderer_draw(
            handle,
            term.unsafeHandle,
            texPtr,
            UInt32(viewport.width),
            UInt32(viewport.height),
            time
        )
    }
}
