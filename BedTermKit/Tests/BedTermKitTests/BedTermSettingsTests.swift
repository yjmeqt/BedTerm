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

    @Test("retired Rust-UI experiment keys are swept on init")
    func sweepsRetiredExperimentKeys() throws {
        let defaults = try makeDefaults()
        defaults.set(true, forKey: "settings.useRustHostsList")
        defaults.set(true, forKey: "settings.useRustConnectForm")
        _ = BedTermSettings(defaults: defaults)
        #expect(defaults.object(forKey: "settings.useRustHostsList") == nil)
        #expect(defaults.object(forKey: "settings.useRustConnectForm") == nil)
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

    @Test("command blocks toggle persists")
    func commandBlocksTogglePersists() throws {
        let defaults = try makeDefaults()
        let settings = BedTermSettings(defaults: defaults)
        settings.showCommandBlocks = false

        let reloaded = BedTermSettings(defaults: defaults)
        #expect(!reloaded.showCommandBlocks)
    }

    @Test("migrates old shell-integration key when on")
    func migratesOldShellKeyWhenOn() throws {
        let defaults = try makeDefaults()
        // Simulate pre-migration state: old key exists and is true.
        defaults.set(true, forKey: "settings.installShellIntegrationOnConnect")
        let settings = BedTermSettings(defaults: defaults)
        #expect(settings.showCommandBlocks)
        #expect(defaults.object(forKey: "settings.installShellIntegrationOnConnect") == nil)
    }

    @Test("migrates old shell-integration key when off")
    func migratesOldShellKeyWhenOff() throws {
        let defaults = try makeDefaults()
        defaults.set(false, forKey: "settings.installShellIntegrationOnConnect")
        let settings = BedTermSettings(defaults: defaults)
        // User had explicitly disabled shell integration; blocks follow.
        #expect(!settings.showCommandBlocks)
        #expect(defaults.object(forKey: "settings.installShellIntegrationOnConnect") == nil)
    }

    @Test("migration respects new key when old key is absent")
    func migrationRespectsNewKey() throws {
        let defaults = try makeDefaults()
        defaults.set(false, forKey: "settings.showCommandBlocks")
        let settings = BedTermSettings(defaults: defaults)
        #expect(!settings.showCommandBlocks)
    }
}
