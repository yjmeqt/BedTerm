import Metal
import XCTest

@testable import BedTermKit

/// Renders each Plan A fixture through the Metal renderer into an offscreen
/// texture and asserts the result is non-empty. True pixel-vs-SwiftTerm
/// parity is deferred to Plan B2 (SwiftTerm renders into a UIView's CALayer
/// and offscreening it requires a separate snapshot harness).
///
/// The value of this sweep: if any regression makes `bridge.draw` produce
/// an empty texture, this test catches it per-fixture with a clear error.
final class TerminalRendererParityTests: XCTestCase {
    private let fixtures = [
        "00_ascii_hello",
        "01_ls_color_always",
        "02_clear_then_prompt",
        "03_cursor_addressing",
        "04_utf8_mixed"
    ]

    func testFixturesProduceNonEmptyRender() throws {
        guard let device = MTLCreateSystemDefaultDevice() else {
            throw XCTSkip("no Metal device")
        }
        guard let queue = device.makeCommandQueue() else {
            throw XCTSkip("no command queue")
        }
        guard let bridge = RendererBridge(device: device, queue: queue) else {
            XCTFail("bridge init returned nil")
            return
        }

        let desc = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm,
            width: 1024,
            height: 512,
            mipmapped: false
        )
        desc.usage = [.shaderRead, .renderTarget]

        for name in fixtures {
            try renderFixtureAndAssert(name: name, device: device, queue: queue, bridge: bridge, desc: desc)
        }
    }

    private func renderFixtureAndAssert(
        name: String,
        device: MTLDevice,
        queue: MTLCommandQueue,
        bridge: RendererBridge,
        desc: MTLTextureDescriptor
    ) throws {
        guard let url = Self.fixtureURL(named: name) else {
            XCTFail("missing fixture \(name).bin in test bundle")
            return
        }
        let payload = try Data(contentsOf: url)

        let term = TerminalCore(cols: 80, rows: 24)
        term.feed(payload)

        guard let tex = device.makeTexture(descriptor: desc) else {
            XCTFail("texture allocation failed for \(name)")
            return
        }
        let rc = bridge.draw(
            term: term,
            into: tex,
            viewport: CGSize(width: 1024, height: 512),
            time: 0
        )
        XCTAssertEqual(rc, 0, "metal draw failed on \(name) (rc=\(rc))")

        // Drain GPU work before reading back pixels.
        let drain = queue.makeCommandBuffer()
        drain?.commit()
        drain?.waitUntilCompleted()

        let bytesPerRow = 1024 * 4
        var bytes = [UInt8](repeating: 0, count: bytesPerRow * 512)
        tex.getBytes(
            &bytes,
            bytesPerRow: bytesPerRow,
            from: MTLRegionMake2D(0, 0, 1024, 512),
            mipmapLevel: 0
        )

        // Count non-zero R/G/B bytes (skip alpha lane — clear colour sets
        // alpha=255 which would make any cleared texture trivially pass).
        var nonZeroRGB = 0
        for pixel in stride(from: 0, to: bytes.count, by: 4) {
            if bytes[pixel] != 0 || bytes[pixel + 1] != 0 || bytes[pixel + 2] != 0 {
                nonZeroRGB += 1
            }
        }
        print("[ParitySweep] \(name): nonZeroRGB=\(nonZeroRGB)")
        XCTAssertGreaterThan(
            nonZeroRGB, 100,
            "renderer produced empty image for \(name) (nonZeroRGB=\(nonZeroRGB))"
        )
    }

    /// Resolve fixture URL using the same pattern as `TerminalCoreShadowTests`:
    /// the Fixtures directory is shipped as a folder reference, so files sit
    /// under `<bundle>/Fixtures/byte_streams/<name>.bin`.
    private static func fixtureURL(named name: String) -> URL? {
        let bundle = Bundle(for: TerminalRendererParityTests.self)
        let dir = bundle.bundleURL.appendingPathComponent("Fixtures/byte_streams")
        let candidate = dir.appendingPathComponent("\(name).bin")
        if FileManager.default.fileExists(atPath: candidate.path) {
            return candidate
        }
        // Fallback: bundle resource lookup (in case Xcode flattens the folder).
        if let url = bundle.url(
            forResource: name,
            withExtension: "bin",
            subdirectory: "Fixtures/byte_streams"
        ) {
            return url
        }
        return bundle.url(forResource: name, withExtension: "bin")
    }
}
