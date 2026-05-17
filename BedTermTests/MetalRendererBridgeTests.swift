import XCTest
import Metal
@testable import BedTermKit

final class MetalRendererBridgeTests: XCTestCase {
    func testNewSetFontFree() throws {
        guard let device = MTLCreateSystemDefaultDevice() else {
            throw XCTSkip("no Metal device on this host")
        }
        guard let queue = device.makeCommandQueue() else {
            throw XCTSkip("no command queue")
        }
        guard let bridge = RendererBridge(device: device, queue: queue) else {
            XCTFail("bridge init returned nil")
            return
        }
        bridge.setFont(pointSize: 14, scale: 3)
        // No assertion — verifying no crash on init/setFont/dealloc.
        _ = bridge // keep alive
    }

    func testDrawIntoOffscreenTexture() throws {
        guard let device = MTLCreateSystemDefaultDevice() else { throw XCTSkip("no Metal device") }
        guard let queue = device.makeCommandQueue() else { throw XCTSkip("no command queue") }
        guard let bridge = RendererBridge(device: device, queue: queue) else {
            XCTFail("bridge init returned nil"); return
        }
        let term = TerminalCore(cols: 80, rows: 24)
        term.feed(Data("hi\n".utf8))

        let desc = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm, width: 512, height: 512, mipmapped: false
        )
        desc.usage = [.shaderRead, .renderTarget]
        guard let texture = device.makeTexture(descriptor: desc) else {
            XCTFail("texture allocation failed"); return
        }
        let rc = bridge.draw(term: term, into: texture, viewport: CGSize(width: 512, height: 512), time: 0)
        XCTAssertEqual(rc, 0)
    }
}
