import Foundation
import Metal
import Testing

@testable import BedTermKit

/// Renders each fixture through the Metal renderer into an offscreen
/// texture and asserts the result is non-empty. If any regression makes
/// `bridge.draw` produce an empty texture, this test catches it
/// per-fixture with a clear error.
@Suite("TerminalRenderer parity sweep")
struct TerminalRendererParityTests {
    static let fixtures = [
        "00_ascii_hello",
        "01_ls_color_always",
        "02_clear_then_prompt",
        "03_cursor_addressing",
        "04_utf8_mixed"
    ]

    @Test("fixtures produce non-empty render", arguments: fixtures)
    func fixturesProduceNonEmptyRender(name: String) throws {
        let device = try #require(MTLCreateSystemDefaultDevice(), "no Metal device")
        let queue = try #require(device.makeCommandQueue(), "no command queue")
        let bridge = try #require(RendererBridge(device: device, queue: queue), "bridge init returned nil")

        let desc = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm,
            width: 1024,
            height: 512,
            mipmapped: false
        )
        desc.usage = [.shaderRead, .renderTarget]

        let url = try #require(Self.fixtureURL(named: name), "missing fixture \(name).bin in test bundle")
        let payload = try Data(contentsOf: url)

        let term = TerminalCore(cols: 80, rows: 24)
        term.feed(payload)

        let tex = try #require(device.makeTexture(descriptor: desc), "texture allocation failed for \(name)")
        let rc = bridge.draw(
            term: term,
            into: tex,
            viewport: CGSize(width: 1024, height: 512),
            time: 0
        )
        #expect(rc == 0, "metal draw failed on \(name) (rc=\(rc))")

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
        #expect(
            nonZeroRGB > 100,
            "renderer produced empty image for \(name) (nonZeroRGB=\(nonZeroRGB))"
        )
    }

    /// Fixtures are shipped as a folder-reference via `.copy("Fixtures")`
    /// in `Package.swift`, so look them up through `Bundle.module`.
    private static func fixtureURL(named name: String) -> URL? {
        let bundle = Bundle.module
        if let url = bundle.url(
            forResource: name,
            withExtension: "bin",
            subdirectory: "Fixtures/byte_streams"
        ) {
            return url
        }
        let candidate = bundle.bundleURL.appendingPathComponent("Fixtures/byte_streams/\(name).bin")
        if FileManager.default.fileExists(atPath: candidate.path) {
            return candidate
        }
        return bundle.url(forResource: name, withExtension: "bin")
    }
}
