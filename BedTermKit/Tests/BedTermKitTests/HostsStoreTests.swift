import BedTermIOS
import Foundation
import Testing

@testable import BedTermKit

@Suite("HostsStore", .serialized)
struct HostsStoreTests {
    init() {
        // Each suite instance routes Rust-side reads/writes to a fresh
        // per-test service / orderKey pair so the simulator Keychain
        // and the production UserDefaults entry stay clean.
        let suffix = UUID().uuidString
        let service = "com.applovin.yi.bedterm.tests.savedHosts.\(suffix)"
        let orderKey = "tests.hosts.order.\(suffix)"
        service.withCString { svcPtr in
            orderKey.withCString { ordPtr in
                bt_ios_hosts_set_test_service(svcPtr, ordPtr)
            }
        }
    }

    private func makeStore() -> HostsStore {
        HostsStore()
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
        try store.save(orphan)
        // Wipe just the order index — blob stays alive. Mimics the
        // "Keychain survived a UserDefaults wipe" scenario.
        bt_ios_hosts_test_clear_order()
        let listed = store.list()
        #expect(listed.map(\.id) == [orphan.id])
        // After reconcile the order is persisted again so subsequent
        // `list()` calls return the entry without re-reconciling.
        let listed2 = store.list()
        #expect(listed2.map(\.id) == [orphan.id])
    }
}
