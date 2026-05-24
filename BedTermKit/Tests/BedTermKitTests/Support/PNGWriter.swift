import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Writes BGRA8 pixel data (from Metal `.bgra8Unorm` textures) to PNG files.
enum PNGWriter {
    enum Error: Swift.Error {
        case imageCreationFailed
        case destinationCreationFailed
        case finalizeFailed
    }

    /// Convert BGRA pixel buffer to a PNG file.
    /// - Parameters:
    ///   - bgraPixels: Raw BGRA bytes from `MTLTexture.getBytes`
    ///   - width: Image width in pixels
    ///   - height: Image height in pixels
    ///   - url: Destination file URL
    static func writeBGRA8(
        bgraPixels: [UInt8],
        width: Int,
        height: Int,
        to url: URL
    ) throws {
        // BGRA → RGBA byte swap (matching Rust png.rs).
        var rgba = bgraPixels
        for i in stride(from: 0, to: rgba.count, by: 4) {
            let tmp = rgba[i]
            rgba[i] = rgba[i + 2]
            rgba[i + 2] = tmp
        }

        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue)
        let data = Data(bytes: rgba, count: rgba.count) as CFData

        guard let provider = CGDataProvider(data: data),
            let cgImage = CGImage(
                width: width,
                height: height,
                bitsPerComponent: 8,
                bitsPerPixel: 32,
                bytesPerRow: width * 4,
                space: colorSpace,
                bitmapInfo: bitmapInfo,
                provider: provider,
                decode: nil,
                shouldInterpolate: false,
                intent: .defaultIntent
            )
        else { throw Error.imageCreationFailed }

        guard
            let dest = CGImageDestinationCreateWithURL(
                url as CFURL,
                UTType.png.identifier as CFString,
                1,
                nil)
        else { throw Error.destinationCreationFailed }

        CGImageDestinationAddImage(dest, cgImage, nil)
        guard CGImageDestinationFinalize(dest) else {
            throw Error.finalizeFailed
        }
    }
}
