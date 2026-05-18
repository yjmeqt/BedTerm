import XCTest

@testable import BedTermKit

@MainActor
final class BedTermSettingsTests: XCTestCase {
    private func makeDefaults(_ suite: String = UUID().uuidString) throws -> UserDefaults {
        // Each test gets a fresh suite so writes don't leak between cases or
        // into the device's standard defaults.
        UserDefaults().removePersistentDomain(forName: suite)
        return try XCTUnwrap(UserDefaults(suiteName: suite))
    }

    func testDefaultsOnFirstLaunch() throws {
        let settings = BedTermSettings(defaults: try makeDefaults())
        XCTAssertTrue(settings.reserveTopSafeAreaInAltScreen)
        XCTAssertFalse(settings.showCommandBlocks)
    }

    func testTogglePersists() throws {
        let defaults = try makeDefaults()
        let first = BedTermSettings(defaults: defaults)
        first.reserveTopSafeAreaInAltScreen = false

        let second = BedTermSettings(defaults: defaults)
        XCTAssertFalse(second.reserveTopSafeAreaInAltScreen)
    }

    func testToggleRestoresAfterFlip() throws {
        let defaults = try makeDefaults()
        let settings = BedTermSettings(defaults: defaults)
        settings.reserveTopSafeAreaInAltScreen = false
        settings.reserveTopSafeAreaInAltScreen = true

        let reloaded = BedTermSettings(defaults: defaults)
        XCTAssertTrue(reloaded.reserveTopSafeAreaInAltScreen)
    }
}
