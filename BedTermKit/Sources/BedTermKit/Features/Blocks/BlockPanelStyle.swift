import UIKit

/// Tunables for the Warp-style rounded block panel that Rust paints
/// behind each block. Centralised here so `BlockListContainerView` and
/// any future call site read the same numbers.
@MainActor
enum BlockPanelStyle {
    /// Corner radius in points. ~10pt gives the Warp look without
    /// looking childishly bubbly.
    static let cornerRadiusPt: CGFloat = 10

    /// Horizontal inset between the panel's left edge and cell column 0.
    /// Matches the right-side padding too — Rust paints the panel BG
    /// full-width and cells sit indented inside.
    static let cellLeftInsetPt: CGFloat = 12

    /// Resolve the panel BG colour from the design-system token for the
    /// given trait collection (light/dark). Packs into `0xRRGGBBAA`
    /// because the Rust FFI takes a single `u32`.
    static func bgRgba(for traits: UITraitCollection) -> UInt32 {
        let baseName = "ShadcnCard"
        let resolved =
            UIColor(named: baseName, in: .module, compatibleWith: traits)
            ?? UIColor.secondarySystemBackground
        let rgba = resolved.resolvedColor(with: traits)
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        rgba.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        let pack = { (component: CGFloat) -> UInt32 in
            UInt32((max(0, min(1, component)) * 255.0).rounded())
        }
        return (pack(red) << 24) | (pack(green) << 16) | (pack(blue) << 8) | pack(alpha)
    }
}
