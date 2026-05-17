import UIKit
import XCTest

@testable import BedTermKit

final class TerminalPaletteTests: XCTestCase {
    func test_lightModeForegroundOnBackgroundHasUsableContrast() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        XCTAssertGreaterThan(
            luma(palette.defaultBg) - luma(palette.defaultFg), 150,
            "light mode fg/bg contrast must be >150 luma units")
    }

    func test_darkModeForegroundOnBackgroundHasUsableContrast() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        XCTAssertGreaterThan(
            luma(palette.defaultFg) - luma(palette.defaultBg), 150,
            "dark mode fg/bg contrast must be >150 luma units")
    }

    func test_lightAndDarkProduceDistinctDefaults() {
        let light = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        let dark = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        XCTAssertNotEqual(light.defaultFg, dark.defaultFg)
        XCTAssertNotEqual(light.defaultBg, dark.defaultBg)
    }

    func test_lightAnsiIndicesAreDistinguishable() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        // Adjacent ANSI indices must not be the same colour — catches
        // copy-paste errors in the colorset table.
        for idx in 0..<(palette.ansi.count - 1) where palette.ansi[idx] == palette.ansi[idx + 1] {
            XCTFail("Light mode ANSI \(idx) collides with ANSI \(idx + 1)")
        }
    }

    func test_darkAnsiIndicesAreDistinguishable() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        for idx in 0..<(palette.ansi.count - 1) where palette.ansi[idx] == palette.ansi[idx + 1] {
            XCTFail("Dark mode ANSI \(idx) collides with ANSI \(idx + 1)")
        }
    }

    func test_ansiPaletteDiffersAcrossAppearances() {
        let light = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        let dark = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        // Most ANSI slots should differ between appearances; at least 10/16 is plenty
        // even after accounting for slots that are intentionally invariant (ANSI 0, 8).
        let differing = (0..<16).filter { light.ansi[$0] != dark.ansi[$0] }.count
        XCTAssertGreaterThanOrEqual(
            differing, 10,
            "expected most ANSI slots to differ across appearances")
    }

    private func luma(_ component: TerminalPalette.Component) -> Double {
        0.2126 * Double(component.r) + 0.7152 * Double(component.g) + 0.0722 * Double(component.b)
    }
}
