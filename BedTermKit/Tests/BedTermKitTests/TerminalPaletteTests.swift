import Testing
import UIKit

@testable import BedTermKit

@Suite("TerminalPalette")
struct TerminalPaletteTests {
    @Test("light mode foreground/background has usable contrast")
    func lightModeForegroundOnBackgroundHasUsableContrast() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        #expect(
            luma(palette.defaultBg) - luma(palette.defaultFg) > 150,
            "light mode fg/bg contrast must be >150 luma units")
    }

    @Test("dark mode foreground/background has usable contrast")
    func darkModeForegroundOnBackgroundHasUsableContrast() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        #expect(
            luma(palette.defaultFg) - luma(palette.defaultBg) > 150,
            "dark mode fg/bg contrast must be >150 luma units")
    }

    @Test("light and dark produce distinct defaults")
    func lightAndDarkProduceDistinctDefaults() {
        let light = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        let dark = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        #expect(light.defaultFg != dark.defaultFg)
        #expect(light.defaultBg != dark.defaultBg)
    }

    @Test("light ANSI indices are distinguishable")
    func lightAnsiIndicesAreDistinguishable() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        // Adjacent ANSI indices must not be the same colour — catches
        // copy-paste errors in the colorset table.
        for idx in 0..<(palette.ansi.count - 1) {
            #expect(
                palette.ansi[idx] != palette.ansi[idx + 1],
                "Light mode ANSI \(idx) collides with ANSI \(idx + 1)")
        }
    }

    @Test("dark ANSI indices are distinguishable")
    func darkAnsiIndicesAreDistinguishable() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        for idx in 0..<(palette.ansi.count - 1) {
            #expect(
                palette.ansi[idx] != palette.ansi[idx + 1],
                "Dark mode ANSI \(idx) collides with ANSI \(idx + 1)")
        }
    }

    @Test("ANSI palette differs across appearances")
    func ansiPaletteDiffersAcrossAppearances() {
        let light = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        let dark = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        // Most ANSI slots should differ between appearances; at least 10/16 is plenty
        // even after accounting for slots that are intentionally invariant (ANSI 0, 8).
        let differing = (0..<16).filter { light.ansi[$0] != dark.ansi[$0] }.count
        #expect(
            differing >= 10,
            "expected most ANSI slots to differ across appearances")
    }

    private func luma(_ component: TerminalPalette.Component) -> Double {
        0.2126 * Double(component.r) + 0.7152 * Double(component.g) + 0.0722 * Double(component.b)
    }
}
