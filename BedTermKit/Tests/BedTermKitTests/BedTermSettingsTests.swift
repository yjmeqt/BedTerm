import Foundation
import Testing

@testable import BedTermKit

@MainActor
@Suite("BedTermSettings")
struct BedTermSettingsTests {
    private func makeDefaults(_ suite: String = UUID().uuidString) throws -> UserDefaults {
        // Each test gets a fresh suite so writes don't leak between cases or
        // into the device's standard defaults.
        UserDefaults().removePersistentDomain(forName: suite)
        return try #require(UserDefaults(suiteName: suite))
    }

    @Test("defaults on first launch")
    func defaultsOnFirstLaunch() throws {
        let settings = BedTermSettings(defaults: try makeDefaults())
        #expect(settings.reserveTopSafeAreaInAltScreen)
        #expect(!settings.showCommandBlocks)
    }

    @Test("toggle persists across reloads")
    func togglePersists() throws {
        let defaults = try makeDefaults()
        let first = BedTermSettings(defaults: defaults)
        first.reserveTopSafeAreaInAltScreen = false

        let second = BedTermSettings(defaults: defaults)
        #expect(!second.reserveTopSafeAreaInAltScreen)
    }

    @Test("toggle restores after flip")
    func toggleRestoresAfterFlip() throws {
        let defaults = try makeDefaults()
        let settings = BedTermSettings(defaults: defaults)
        settings.reserveTopSafeAreaInAltScreen = false
        settings.reserveTopSafeAreaInAltScreen = true

        let reloaded = BedTermSettings(defaults: defaults)
        #expect(reloaded.reserveTopSafeAreaInAltScreen)
    }
}
