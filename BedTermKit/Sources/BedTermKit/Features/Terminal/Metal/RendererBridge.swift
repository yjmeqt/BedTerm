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
        let qPtr   = Unmanaged.passRetained(queue  as AnyObject).toOpaque()
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
