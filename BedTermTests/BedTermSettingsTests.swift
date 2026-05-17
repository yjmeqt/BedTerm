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
        XCTAssertTrue(settings.autoHideComposerInAltScreen)
        XCTAssertFalse(settings.showCommandBlocks)
    }

    func testTogglePersists() throws {
        let defaults = try makeDefaults()
        let first = BedTermSettings(defaults: defaults)
        first.autoHideComposerInAltScreen = false

        let second = BedTermSettings(defaults: defaults)
        XCTAssertFalse(second.autoHideComposerInAltScreen)
    }

    func testToggleRestoresAfterFlip() throws {
        let defaults = try makeDefaults()
        let settings = BedTermSettings(defaults: defaults)
        settings.autoHideComposerInAltScreen = false
        settings.autoHideComposerInAltScreen = true

        let reloaded = BedTermSettings(defaults: defaults)
        XCTAssertTrue(reloaded.autoHideComposerInAltScreen)
    }
}
