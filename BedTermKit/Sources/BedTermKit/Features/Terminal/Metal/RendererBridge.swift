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

    /// Update the Metal renderer's `MTLLoadAction::Clear` colour. Takes effect
    /// on the next `draw(...)`. Components outside `[0, 1]` are tolerated by
    /// Metal (clamped downstream).
    func setClearColor(red: Float, green: Float, blue: Float, alpha: Float = 1.0) {
        bt_renderer_set_clear_color(handle, red, green, blue, alpha)
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

    /// Draw a caller-provided cell array. Used by Block view: sealed blocks
    /// pass a frozen `GridSnapshot` captured at command-end; running blocks
    /// pass a fresh range snapshot. Pipeline is identical to `draw(term:)`
    /// so ANSI colours / wide chars / underlines render the same way.
    ///
    /// Copies `cells` into a transient C-layout buffer because Swift's
    /// `GridSnapshot.Cell` isn't `@frozen` with explicit C layout —
    /// `withMemoryRebound` between the two types isn't guaranteed safe.
    @discardableResult
    func drawCells(
        _ snapshot: GridSnapshot,
        into texture: MTLTexture,
        viewport: CGSize,
        time: CFTimeInterval
    ) -> Int32 {
        let texPtr = Unmanaged.passUnretained(texture as AnyObject).toOpaque()
        var cBuffer: [CellSnapshot] = snapshot.cells.map {
            CellSnapshot(ch: $0.ch, fg_rgba: $0.fgRGBA, bg_rgba: $0.bgRGBA, flags: $0.flags)
        }
        return cBuffer.withUnsafeMutableBufferPointer { buf in
            bt_renderer_draw_cells(
                handle,
                buf.baseAddress,
                UInt(buf.count),
                snapshot.cols,
                snapshot.rows,
                texPtr,
                UInt32(viewport.width),
                UInt32(viewport.height),
                time
            )
        }
    }

    /// Draw every visible block body in one pass via
    /// `bt_renderer_draw_block_list`. `layout` is a Swift-built table of
    /// `BtBlockLayoutEntry` describing where each block's body sits in the
    /// content (logical pixel) coordinate space; the renderer applies
    /// `scrollOffsetPx` to clip against the texture viewport.
    @discardableResult
    // swiftlint:disable:next function_parameter_count
    func drawBlockList(
        term: TerminalCore,
        into texture: MTLTexture,
        viewport: CGSize,
        scrollOffsetPx: CGFloat,
        layout: [BtBlockLayoutEntry],
        headers: [BtBlockHeaderEntry]
    ) -> Int32 {
        let texPtr = Unmanaged.passUnretained(texture as AnyObject).toOpaque()
        return layout.withUnsafeBufferPointer { lbuf in
            headers.withUnsafeBufferPointer { hbuf in
                bt_renderer_draw_block_list(
                    handle,
                    term.unsafeHandle,
                    texPtr,
                    UInt32(viewport.width),
                    UInt32(viewport.height),
                    Float(scrollOffsetPx),
                    lbuf.baseAddress,
                    UInt(lbuf.count),
                    hbuf.baseAddress,
                    UInt(hbuf.count)
                )
            }
        }
    }

    /// Push current UI font sizes (resolved from Dynamic Type) into the
    /// renderer so the header band uses the right pixel sizes when it
    /// rasterizes command + subtitle glyphs. Values must be in pixels
    /// (point × screen scale).
    func setUIFontSizes(subheadlinePx: Float, caption2Px: Float, scale: Float) {
        _ = bt_renderer_set_ui_font_sizes_px(handle, subheadlinePx, caption2Px, scale)
    }
}
