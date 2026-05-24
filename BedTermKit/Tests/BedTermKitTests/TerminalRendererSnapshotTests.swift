import Foundation
import Metal
import Testing

@testable import BedTermKit

/// Renders fixture byte streams through the iOS Metal renderer at
/// device-specific dimensions and writes PNG snapshots for comparison
/// with the macOS `bedterm-render` CLI output.
///
/// Run via:
/// ```sh
/// RENDER_OUTPUT_DIR=/tmp/renders-cross-platform/ios \
///   worktree-ios-dev-tool test \
///   --only-testing BedTermKitTests/TerminalRendererSnapshotTests
/// ```
@Suite("TerminalRenderer Snapshot (iOS)")
struct TerminalRendererSnapshotTests {

    // MARK: - Device presets (mirrors Rust device_presets.rs)

    private struct DevicePreset {
        let name: String
        let viewportPt: CGSize
        let scale: CGFloat

        static let iphone17 = DevicePreset(
            name: "iphone17", viewportPt: CGSize(width: 402, height: 874), scale: 3)
        static let iphone17Promax = DevicePreset(
            name: "iphone17-promax", viewportPt: CGSize(width: 440, height: 956), scale: 3)
        static let iphone16 = DevicePreset(
            name: "iphone16", viewportPt: CGSize(width: 393, height: 852), scale: 3)
        static let iphone15Promax = DevicePreset(
            name: "iphone15-promax", viewportPt: CGSize(width: 430, height: 932), scale: 3)
        static let iphone14 = DevicePreset(
            name: "iphone14", viewportPt: CGSize(width: 390, height: 844), scale: 3)
        static let iphoneSE3 = DevicePreset(
            name: "iphone-se3", viewportPt: CGSize(width: 375, height: 667), scale: 2)
        static let ipadPro13 = DevicePreset(
            name: "ipad-pro13", viewportPt: CGSize(width: 1032, height: 1376), scale: 2)
        static let ipadPro11 = DevicePreset(
            name: "ipad-pro11", viewportPt: CGSize(width: 834, height: 1210), scale: 2)
        static let ipadAir13 = DevicePreset(
            name: "ipad-air13", viewportPt: CGSize(width: 1024, height: 1366), scale: 2)
        static let ipadAir11 = DevicePreset(
            name: "ipad-air11", viewportPt: CGSize(width: 820, height: 1180), scale: 2)
        static let ipadMini = DevicePreset(
            name: "ipad-mini", viewportPt: CGSize(width: 744, height: 1133), scale: 2)
        static let mac = DevicePreset(
            name: "mac", viewportPt: CGSize(width: 1200, height: 800), scale: 2)
    }

    // MARK: - Palette (hardcoded to match Rust CLI, no asset catalog dependency)

    private enum SnapshotPalette: String, CaseIterable, CustomTestStringConvertible {
        case bedtermDark = "bedterm-dark"
        case bedtermLight = "bedterm-light"

        var testDescription: String { rawValue }
    }

    private static func paletteFor(_ name: SnapshotPalette) -> TerminalPalette {
        switch name {
        case .bedtermDark:
            TerminalPalette(
                defaultFg: .init(r: 0xCC, g: 0xCC, b: 0xCC),
                defaultBg: .init(r: 0x00, g: 0x00, b: 0x00),
                ansi: [
                    .init(r: 0x00, g: 0x00, b: 0x00),
                    .init(r: 0xCC, g: 0x33, b: 0x33),
                    .init(r: 0x33, g: 0xCC, b: 0x33),
                    .init(r: 0xCC, g: 0xCC, b: 0x33),
                    .init(r: 0x33, g: 0x66, b: 0xCC),
                    .init(r: 0xCC, g: 0x33, b: 0xCC),
                    .init(r: 0x33, g: 0xCC, b: 0xCC),
                    .init(r: 0xCC, g: 0xCC, b: 0xCC),
                    .init(r: 0x55, g: 0x55, b: 0x55),
                    .init(r: 0xFF, g: 0x66, b: 0x66),
                    .init(r: 0x66, g: 0xFF, b: 0x66),
                    .init(r: 0xFF, g: 0xFF, b: 0x66),
                    .init(r: 0x66, g: 0x99, b: 0xFF),
                    .init(r: 0xFF, g: 0x66, b: 0xFF),
                    .init(r: 0x66, g: 0xFF, b: 0xFF),
                    .init(r: 0xFF, g: 0xFF, b: 0xFF),
                ])
        case .bedtermLight:
            TerminalPalette(
                defaultFg: .init(r: 0x1A, g: 0x1A, b: 0x1A),
                defaultBg: .init(r: 0xFF, g: 0xFF, b: 0xFF),
                ansi: [
                    .init(r: 0x00, g: 0x00, b: 0x00),
                    .init(r: 0xC9, g: 0x1B, b: 0x00),
                    .init(r: 0x00, g: 0xA0, b: 0x00),
                    .init(r: 0xA1, g: 0x84, b: 0x00),
                    .init(r: 0x00, g: 0x59, b: 0xCB),
                    .init(r: 0xB0, g: 0x00, b: 0xB0),
                    .init(r: 0x00, g: 0xA1, b: 0xA1),
                    .init(r: 0xBE, g: 0xBE, b: 0xBE),
                    .init(r: 0x55, g: 0x55, b: 0x55),
                    .init(r: 0xFF, g: 0x3F, b: 0x1F),
                    .init(r: 0x00, g: 0xCB, b: 0x00),
                    .init(r: 0xC8, g: 0xA8, b: 0x00),
                    .init(r: 0x00, g: 0x71, b: 0xFF),
                    .init(r: 0xE0, g: 0x00, b: 0xE0),
                    .init(r: 0x00, g: 0xC8, b: 0xC8),
                    .init(r: 0x1A, g: 0x1A, b: 0x1A),
                ])
        }
    }

    // MARK: - Newline normalization

    /// Convert bare LF to CR+LF so piped script output renders identically
    /// to PTY-recorded sessions (where the tty driver does this translation).
    /// Mirrors `normalize_newlines` in context.rs.
    private static func normalizeNewlines(_ data: Data) -> Data {
        var out = Data(capacity: data.count + data.count / 10)
        var prevCR = false
        for byte in data {
            if byte == 0x0A && !prevCR {
                out.append(0x0D)
            }
            out.append(byte)
            prevCR = byte == 0x0D
        }
        return out
    }

    // MARK: - Fixture loading

    private struct FixtureSource: CustomTestStringConvertible {
        let name: String
        let url: URL
        var testDescription: String { name }
    }

    /// Discover all .bin fixtures in the test bundle's Fixtures/byte_streams/.
    private static func loadFixtures() -> [FixtureSource] {
        var fixtures: [FixtureSource] = []

        let bundle = Bundle.module
        // Primary path for .copy("Fixtures") resource
        let dirURL = bundle.bundleURL
            .appendingPathComponent("Fixtures/byte_streams")
        if let enumerator = FileManager.default.enumerator(
            at: dirURL,
            includingPropertiesForKeys: [.isRegularFileKey]
        ) {
            for case let url as URL in enumerator where url.pathExtension == "bin" {
                let name = url.deletingPathExtension().lastPathComponent
                fixtures.append(FixtureSource(name: name, url: url))
            }
        }

        // Fallback: look up individual files in bundle resource path
        if fixtures.isEmpty {
            if let resURL = bundle.resourceURL?
                .appendingPathComponent("Fixtures/byte_streams")
            {
                if let enumerator = FileManager.default.enumerator(
                    at: resURL,
                    includingPropertiesForKeys: [.isRegularFileKey]
                ) {
                    for case let url as URL in enumerator
                    where url.pathExtension == "bin" {
                        let name = url.deletingPathExtension().lastPathComponent
                        fixtures.append(FixtureSource(name: name, url: url))
                    }
                }
            }
        }

        return fixtures.sorted { $0.name < $1.name }
    }

    // MARK: - Output directory

    private static var outputDir: URL {
        if let env = ProcessInfo.processInfo.environment["RENDER_OUTPUT_DIR"] {
            return URL(fileURLWithPath: env)
        }
        return URL(fileURLWithPath: "/tmp/renders-cross-platform/ios/")
    }

    // MARK: - Test parameters

    private struct SnapshotParams: CustomTestStringConvertible {
        let fixture: FixtureSource
        let device: DevicePreset
        let palette: SnapshotPalette
        var testDescription: String { "\(fixture.name)-\(device.name)-\(palette)" }
    }

    private static let devices: [DevicePreset] = [.iphone17, .ipadPro13]
    private static let palettes = SnapshotPalette.allCases
    private static let fixtures = loadFixtures()

    private static func buildParams() -> [SnapshotParams] {
        fixtures.flatMap { f in
            devices.flatMap { d in
                palettes.map { p in
                    SnapshotParams(fixture: f, device: d, palette: p)
                }
            }
        }
    }

    // MARK: - Tests

    @Test("render snapshot", arguments: buildParams())
    private func renderSnapshot(param: SnapshotParams) throws {
        let device = try #require(MTLCreateSystemDefaultDevice(), "no Metal device")
        let queue = try #require(device.makeCommandQueue(), "no command queue")
        let bridge = try #require(
            RendererBridge(device: device, queue: queue), "bridge init failed")

        let preset = param.device

        // 1. Font + cell metrics → derive cols/rows
        bridge.setFont(pointSize: 14, scale: preset.scale)
        let cellPt = bridge.cellSizeInPoints(scale: preset.scale)
        let cols = Int((preset.viewportPt.width / cellPt.width).rounded(.down))
        let rows = Int((preset.viewportPt.height / cellPt.height).rounded(.down))
        guard cols > 0, rows > 0 else {
            Issue.record("zero cols/rows for \(preset.name): cellPt=\(cellPt)")
            return
        }

        // 2. Palette + clear color
        let palette = Self.paletteFor(param.palette)
        let bg = palette.defaultBg
        bridge.setClearColor(
            red: Float(bg.r) / 255,
            green: Float(bg.g) / 255,
            blue: Float(bg.b) / 255,
            alpha: 1.0)

        // 3. Terminal + feed fixture (with LF → CR+LF normalization)
        let rawPayload = try Data(contentsOf: param.fixture.url)
        let payload = Self.normalizeNewlines(rawPayload)
        let term = TerminalCore(cols: cols, rows: rows)
        term.setPalette(palette)
        term.feed(payload)

        // 4. Offscreen texture at device pixel dimensions
        let texWidth = Int((preset.viewportPt.width * preset.scale).rounded())
        let texHeight = Int((preset.viewportPt.height * preset.scale).rounded())
        let desc = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm,
            width: texWidth,
            height: texHeight,
            mipmapped: false
        )
        desc.usage = [.shaderRead, .renderTarget]
        let texture = try #require(
            device.makeTexture(descriptor: desc), "texture allocation failed")

        // 5. Draw
        let rc = bridge.draw(
            term: term,
            into: texture,
            viewport: CGSize(width: texWidth, height: texHeight),
            time: 0
        )
        #expect(rc == 0, "draw failed on \(param.testDescription)")

        // 6. Drain GPU
        let drain = queue.makeCommandBuffer()
        drain?.commit()
        drain?.waitUntilCompleted()

        // 7. Read back pixels
        let bytesPerRow = texWidth * 4
        var pixels = [UInt8](repeating: 0, count: bytesPerRow * texHeight)
        texture.getBytes(
            &pixels,
            bytesPerRow: bytesPerRow,
            from: MTLRegionMake2D(0, 0, texWidth, texHeight),
            mipmapLevel: 0
        )

        // 8. Verify non-empty
        var nonZero = 0
        for i in stride(from: 0, to: pixels.count, by: 4) {
            if pixels[i] != 0 || pixels[i + 1] != 0 || pixels[i + 2] != 0 {
                nonZero += 1
            }
        }
        #expect(nonZero > 100, "empty render for \(param.testDescription)")

        // 9. Write PNG
        let dir = Self.outputDir
        try FileManager.default.createDirectory(
            at: dir, withIntermediateDirectories: true)
        let filename =
            "\(param.fixture.name)-\(preset.name)-\(param.palette.rawValue)-grid.png"
        let url = dir.appendingPathComponent(filename)
        try PNGWriter.writeBGRA8(
            bgraPixels: pixels, width: texWidth, height: texHeight, to: url)

        print(
            "[Snapshot] \(param.testDescription): cols=\(cols) rows=\(rows) cell=\(cellPt) tex=\(texWidth)x\(texHeight)"
        )
    }
}
