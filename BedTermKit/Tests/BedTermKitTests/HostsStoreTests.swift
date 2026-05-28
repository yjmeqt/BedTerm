import BedTermIOS
import Foundation
import Testing

@testable import BedTermKit

@Suite("HostsStore", .serialized)
struct HostsStoreTests {
    init() {
        let suffix = UUID().uuidString
        let service = "com.applovin.yi.bedterm.tests.savedHosts.\(suffix)"
        let orderKey = "tests.hosts.order.\(suffix)"
        service.withCString { svcPtr in
            orderKey.withCString { ordPtr in
                bt_ios_hosts_set_test_service(svcPtr, ordPtr)
            }
        }
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

    private func saveEntry(_ entry: SavedHost) throws {
        let data = try JSONEncoder().encode(entry)
        let ok = entry.id.uuidString.withCString { idPtr in
            data.withUnsafeBytes { raw -> Bool in
                let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                return bt_ios_hosts_save_blob(idPtr, base, UInt(raw.count))
            }
        }
        #expect(ok)
    }

    private func listEntries() -> [SavedHost] {
        guard let snapshotPtr = bt_ios_hosts_snapshot_json() else { return [] }
        defer { bt_ios_hosts_free_string(snapshotPtr) }
        let json = String(cString: snapshotPtr)
        guard let data = json.data(using: .utf8),
            let items = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        var out: [SavedHost] = []
        for item in items {
            guard let idStr = item["id"] as? String,
                let uuid = UUID(uuidString: idStr)
            else { continue }
            if let entry = loadEntry(id: uuid) {
                out.append(entry)
            }
        }
        return out
    }

    private func loadEntry(id: UUID) -> SavedHost? {
        guard let ptr = id.uuidString.withCString({ bt_ios_hosts_load_json($0) }) else {
            return nil
        }
        defer { bt_ios_hosts_free_string(ptr) }
        guard let data = String(cString: ptr).data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(SavedHost.self, from: data)
    }

    private func deleteEntry(id: UUID) {
        id.uuidString.withCString { bt_ios_hosts_delete($0) }
    }

    @Test("save then list returns the saved entry with all fields preserved")
    func roundTrip() throws {
        let entry = makeHost()
        try saveEntry(entry)
        let listed = listEntries()
        #expect(listed == [entry])
    }

    @Test("list preserves insertion order")
    func orderPreserved() throws {
        let h1 = makeHost(label: "a", host: "a.example.com")
        let h2 = makeHost(label: "b", host: "b.example.com")
        let h3 = makeHost(label: "c", host: "c.example.com")
        try saveEntry(h1)
        try saveEntry(h2)
        try saveEntry(h3)
        #expect(listEntries().map(\.id) == [h1.id, h2.id, h3.id])
    }

    @Test("save on an existing id updates without changing position")
    func updateInPlace() throws {
        let h1 = makeHost(label: "a", host: "a.example.com")
        let h2 = makeHost(label: "b", host: "b.example.com")
        try saveEntry(h1)
        try saveEntry(h2)

        var updated = h1
        updated.label = "a-prime"
        try saveEntry(updated)

        let listed = listEntries()
        #expect(listed.map(\.id) == [h1.id, h2.id])
        #expect(listed.first?.label == "a-prime")
    }

    @Test("delete removes both the index entry and the Keychain item")
    func deleteRemovesEntry() throws {
        let entry = makeHost()
        try saveEntry(entry)
        deleteEntry(id: entry.id)
        #expect(listEntries().isEmpty)
        #expect(loadEntry(id: entry.id) == nil)
    }

    @Test("list reconciles orphan Keychain items when the index is empty")
    func reconcileOrphans() throws {
        let orphan = makeHost(label: "orphan")
        try saveEntry(orphan)
        bt_ios_hosts_test_clear_order()
        let listed = listEntries()
        #expect(listed.map(\.id) == [orphan.id])
        let listed2 = listEntries()
        #expect(listed2.map(\.id) == [orphan.id])
    }
}
