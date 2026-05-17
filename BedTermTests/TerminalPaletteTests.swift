import UIKit
import XCTest

@testable import BedTermKit

final class TerminalPaletteTests: XCTestCase {
    func test_lightModeBackgroundIsBrighterThanForeground() {
        let traits = UITraitCollection(userInterfaceStyle: .light)
        let palette = TerminalPalette.resolve(for: traits)
        XCTAssertGreaterThan(
            luma(palette.defaultBg), luma(palette.defaultFg),
            "light bg must be brighter than fg"
        )
    }

    func test_darkModeBackgroundIsDarkerThanForeground() {
        let traits = UITraitCollection(userInterfaceStyle: .dark)
        let palette = TerminalPalette.resolve(for: traits)
        XCTAssertLessThan(
            luma(palette.defaultBg), luma(palette.defaultFg),
            "dark bg must be darker than fg"
        )
    }

    func test_lightAndDarkProduceDistinctDefaults() {
        let light = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .light))
        let dark = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        XCTAssertNotEqual(light.defaultFg, dark.defaultFg)
        XCTAssertNotEqual(light.defaultBg, dark.defaultBg)
    }

    func test_ansiHasSixteenEntries() {
        let palette = TerminalPalette.resolve(for: UITraitCollection(userInterfaceStyle: .dark))
        XCTAssertEqual(palette.ansi.count, 16)
    }

    func test_lightRedIsDifferentFromDarkRed() {
        let lightRed = TerminalPalette.resolve(
            for: UITraitCollection(userInterfaceStyle: .light)
        ).ansi[1]
        let darkRed = TerminalPalette.resolve(
            for: UITraitCollection(userInterfaceStyle: .dark)
        ).ansi[1]
        XCTAssertNotEqual(lightRed, darkRed, "ANSI red should differ between appearances")
    }

    private func luma(_ component: TerminalPalette.Component) -> Double {
        0.2126 * Double(component.r) + 0.7152 * Double(component.g) + 0.0722 * Double(component.b)
    }
}
