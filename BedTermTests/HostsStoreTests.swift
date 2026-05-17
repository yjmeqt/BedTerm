import Foundation
import Testing

@testable import BedTermKit

@Suite("HostsStore")
struct HostsStoreTests {
    private let testService = "com.applovin.yi.bedterm.tests.savedHosts"
    private let legacyService = "com.applovin.yi.bedterm.tests.savedHosts.legacy"
    private let orderKey = "tests.hosts.order"
    private let migrationKey = "tests.hosts.legacyMigrationDone"
    private let defaults: UserDefaults

    init() {
        // Each test instance gets its own ephemeral UserDefaults suite and its
        // own Keychain services, so suites do not bleed into each other.
        let suite = "BedTermTests.HostsStore." + UUID().uuidString
        defaults = UserDefaults(suiteName: suite) ?? .standard
        defaults.removePersistentDomain(forName: suite)
        for account in Keychain.allAccounts(service: testService) {
            Keychain.delete(service: testService, account: account)
        }
        Keychain.delete(service: legacyService, account: "default")
    }

    private func makeStore() -> HostsStore {
        HostsStore(
            service: testService,
            orderKey: orderKey,
            migrationKey: migrationKey,
            defaults: defaults,
            legacy: CredentialsStore(service: legacyService)
        )
    }

    private func makeHost(
        label: String = "prod", host: String = "10.0.0.5", port: Int = 22, user: String = "deploy"
    ) -> SavedHost {
        SavedHost(
            label: label,
            credential: HostCredential(
                host: host, port: port, username: user,
                auth: .privateKey(Data("KEY".utf8), passphrase: "p")
            )
        )
    }

    @Test("save then list returns the saved entry with all fields preserved")
    func roundTrip() throws {
        let store = makeStore()
        let entry = makeHost()
        try store.save(entry)
        let listed = store.list()
        #expect(listed == [entry])
    }

    @Test("list preserves insertion order")
    func orderPreserved() throws {
        let store = makeStore()
        let h1 = makeHost(label: "a", host: "a.example.com")
        let h2 = makeHost(label: "b", host: "b.example.com")
        let h3 = makeHost(label: "c", host: "c.example.com")
        try store.save(h1)
        try store.save(h2)
        try store.save(h3)
        #expect(store.list().map(\.id) == [h1.id, h2.id, h3.id])
    }

    @Test("save on an existing id updates without changing position")
    func updateInPlace() throws {
        let store = makeStore()
        let h1 = makeHost(label: "a", host: "a.example.com")
        let h2 = makeHost(label: "b", host: "b.example.com")
        try store.save(h1)
        try store.save(h2)

        var updated = h1
        updated.label = "a-prime"
        try store.save(updated)

        let listed = store.list()
        #expect(listed.map(\.id) == [h1.id, h2.id])
        #expect(listed.first?.label == "a-prime")
    }

    @Test("delete removes both the index entry and the Keychain item")
    func deleteRemovesEntry() throws {
        let store = makeStore()
        let entry = makeHost()
        try store.save(entry)
        store.delete(id: entry.id)
        #expect(store.list().isEmpty)
        #expect(throws: KeychainError.notFound) { try store.load(id: entry.id) }
    }

    @Test("list reconciles orphan Keychain items when the index is empty")
    func reconcileOrphans() throws {
        let store = makeStore()
        let orphan = makeHost(label: "orphan")
        let data = try JSONEncoder().encode(orphan)
        try Keychain.save(service: testService, account: orphan.id.uuidString, data: data)
        // Note: no UserDefaults order entry written.
        let listed = store.list()
        #expect(listed.map(\.id) == [orphan.id])
        // After reconcile the order is persisted for next launch.
        #expect(defaults.array(forKey: orderKey) as? [String] == [orphan.id.uuidString])
    }

    @Test("migration seeds a single entry labeled 'Last connection' from the legacy credential")
    func migrationSeedsLegacyEntry() throws {
        let legacy = HostCredential(host: "legacy.example.com", port: 2222, username: "root", auth: .password("p"))
        try CredentialsStore(service: legacyService).save(legacy)

        let store = makeStore()
        let didMigrate = store.migrateLegacyIfNeeded(label: "Last connection")
        #expect(didMigrate)
        let listed = store.list()
        #expect(listed.count == 1)
        #expect(listed.first?.label == "Last connection")
        #expect(listed.first?.credential == legacy)
        // Legacy item is gone.
        #expect(throws: KeychainError.notFound) { try CredentialsStore(service: legacyService).load() }
    }

    @Test("migration is idempotent")
    func migrationIdempotent() throws {
        let legacy = HostCredential(host: "h", port: 22, username: "u", auth: .password("p"))
        try CredentialsStore(service: legacyService).save(legacy)
        let store = makeStore()
        _ = store.migrateLegacyIfNeeded()
        _ = store.migrateLegacyIfNeeded()
        #expect(store.list().count == 1)
    }

    @Test("migration is a no-op on a fresh install with no legacy item")
    func migrationFreshInstall() {
        let store = makeStore()
        let didMigrate = store.migrateLegacyIfNeeded()
        #expect(!didMigrate)
        #expect(store.list().isEmpty)
    }

    @Test("migration skips when a matching host/port/username is already saved")
    func migrationSkipsDuplicate() throws {
        let legacy = HostCredential(host: "h", port: 22, username: "u", auth: .password("p"))
        try CredentialsStore(service: legacyService).save(legacy)
        let store = makeStore()
        try store.save(SavedHost(label: "existing", credential: legacy))
        let didMigrate = store.migrateLegacyIfNeeded()
        #expect(!didMigrate)
        #expect(store.list().count == 1)
        // Legacy item is cleared even when migration is skipped, so we never run again.
        #expect(throws: KeychainError.notFound) { try CredentialsStore(service: legacyService).load() }
    }
}
