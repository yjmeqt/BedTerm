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
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        ui.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return Component(
            r: UInt8((red * 255.0).rounded().clamped(to: 0...255)),
            g: UInt8((green * 255.0).rounded().clamped(to: 0...255)),
            b: UInt8((blue * 255.0).rounded().clamped(to: 0...255))
        )
    }
}

private extension CGFloat {
    func clamped(to range: ClosedRange<CGFloat>) -> CGFloat {
        Swift.min(Swift.max(self, range.lowerBound), range.upperBound)
    }
}
