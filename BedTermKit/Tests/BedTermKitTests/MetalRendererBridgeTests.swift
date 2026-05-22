import Foundation
import Metal
import Testing

@testable import BedTermKit

@Suite("MetalRendererBridge")
struct MetalRendererBridgeTests {
    @Test("new / setFont / free without crashing")
    func newSetFontFree() throws {
        let device = try #require(MTLCreateSystemDefaultDevice(), "no Metal device on this host")
        let queue = try #require(device.makeCommandQueue(), "no command queue")
        let bridge = try #require(RendererBridge(device: device, queue: queue), "bridge init returned nil")
        bridge.setFont(pointSize: 14, scale: 3)
        // No assertion — verifying no crash on init/setFont/dealloc.
        _ = bridge  // keep alive
    }

    @Test("draw into offscreen texture")
    func drawIntoOffscreenTexture() throws {
        let device = try #require(MTLCreateSystemDefaultDevice(), "no Metal device")
        let queue = try #require(device.makeCommandQueue(), "no command queue")
        let bridge = try #require(RendererBridge(device: device, queue: queue), "bridge init returned nil")
        let term = TerminalCore(cols: 80, rows: 24)
        term.feed(Data("hi\n".utf8))

        let desc = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm, width: 512, height: 512, mipmapped: false
        )
        desc.usage = [.shaderRead, .renderTarget]
        let texture = try #require(device.makeTexture(descriptor: desc), "texture allocation failed")
        let rc = bridge.draw(term: term, into: texture, viewport: CGSize(width: 512, height: 512), time: 0)
        #expect(rc == 0)
    }

    @Test("setClearColor then draw succeeds")
    func setClearColorThenDrawSucceeds() throws {
        let device = try #require(MTLCreateSystemDefaultDevice(), "no Metal device")
        let queue = try #require(device.makeCommandQueue(), "no command queue")
        let bridge = try #require(RendererBridge(device: device, queue: queue), "bridge init returned nil")
        bridge.setFont(pointSize: 14, scale: 2)
        bridge.setClearColor(red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0)

        let term = TerminalCore(cols: 4, rows: 2)
        let desc = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm, width: 64, height: 32, mipmapped: false
        )
        desc.usage = [.renderTarget, .shaderRead]
        let texture = try #require(device.makeTexture(descriptor: desc), "texture allocation failed")
        let rc = bridge.draw(term: term, into: texture, viewport: CGSize(width: 64, height: 32), time: 0)
        #expect(rc == 0, "draw must succeed after setClearColor")
    }
}
