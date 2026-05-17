import UIKit

/// 18-entry terminal palette resolved from the design-system token catalogue
/// (`Tokens.xcassets` → `TerminalForeground`, `TerminalBackground`,
/// `TerminalAnsi0` … `TerminalAnsi15`). Every colour reaches the Metal
/// renderer through here — no hex literals in code.
struct TerminalPalette: Equatable {
    struct Component: Equatable {
        // swiftlint:disable identifier_name
        let r: UInt8
        let g: UInt8
        let b: UInt8
        // swiftlint:enable identifier_name
    }

    let defaultFg: Component
    let defaultBg: Component
    let ansi: [Component]  // exactly 16 entries, index = ANSI colour number

    static func resolve(
        for traits: UITraitCollection,
        bundle: Bundle = .module
    ) -> TerminalPalette {
        let load = { (name: String) -> Component in
            guard let ui = UIColor(named: name, in: bundle, compatibleWith: traits) else {
                preconditionFailure(
                    "Missing colour token: \(name)"
                )
            }
            return Self.toComponent(ui.resolvedColor(with: traits))
        }
        return TerminalPalette(
            defaultFg: load("TerminalForeground"),
            defaultBg: load("TerminalBackground"),
            ansi: (0..<16).map { load("TerminalAnsi\($0)") }
        )
    }

    private static func toComponent(_ ui: UIColor) -> Component {
        // swiftlint:disable identifier_name
        var r: CGFloat = 0
        var g: CGFloat = 0
        var b: CGFloat = 0
        // swiftlint:enable identifier_name
        var alpha: CGFloat = 0
        guard ui.getRed(&r, green: &g, blue: &b, alpha: &alpha) else {
            preconditionFailure(
                """
                Terminal palette token is not in an RGB-compatible colour space — \
                all TerminalAnsi/TerminalForeground/TerminalBackground colorsets must \
                use sRGB.
                """
            )
        }
        return Component(
            r: Self.toByte(r),
            g: Self.toByte(g),
            b: Self.toByte(b)
        )
    }

    private static func toByte(_ component: CGFloat) -> UInt8 {
        let scaled = (component * 255.0).rounded()
        let clamped = Swift.min(Swift.max(scaled, 0), 255)
        return UInt8(clamped)
    }
}
