import BedTermIOS
import Foundation
import Testing

@testable import BedTermKit

@MainActor
@Suite("ConnectionFormViewModel", .serialized)
struct ConnectionFormViewModelTests {
    init() {
        let suffix = UUID().uuidString
        let svc = "com.applovin.yi.bedterm.tests.formvm.\(suffix)"
        let ord = "tests.formvm.order.\(suffix)"
        svc.withCString { sPtr in
            ord.withCString { oPtr in
                bt_ios_hosts_set_test_service(sPtr, oPtr)
            }
        }
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

    @Test("save in add mode appends a new entry with all fields")
    func saveAddMode() throws {
        let vm = ConnectionFormViewModel(mode: .add)
        vm.label = "prod"
        vm.host = "10.0.0.5"
        vm.port = "22"
        vm.username = "deploy"
        vm.password = "hunter2"

        let id = try vm.save()
        let listed = listEntries()
        #expect(listed.count == 1)
        #expect(listed[0].id == id)
        #expect(listed[0].label == "prod")
        #expect(listed[0].credential.host == "10.0.0.5")
        #expect(listed[0].credential.port == 22)
        #expect(listed[0].credential.username == "deploy")
        #expect(listed[0].credential.auth == .password("hunter2"))
    }

    @Test("validation requires host, username, and a port in 1...65535")
    func validation() {
        let vm = ConnectionFormViewModel(mode: .add)
        #expect(throws: ConnectionFormViewModel.FormError.self) { _ = try vm.save() }

        vm.host = "h"
        vm.username = "u"
        vm.password = "p"
        vm.port = "70000"
        #expect(throws: ConnectionFormViewModel.FormError.self) { _ = try vm.save() }

        vm.port = "0"
        #expect(throws: ConnectionFormViewModel.FormError.self) { _ = try vm.save() }

        vm.port = "22"
        #expect((try? vm.save()) != nil)
    }

    @Test("private key auth requires an imported key in add mode")
    func keyAuthRequiresKey() {
        let vm = ConnectionFormViewModel(mode: .add)
        vm.host = "h"; vm.username = "u"; vm.auth = .privateKey
        #expect(throws: ConnectionFormViewModel.FormError.self) { _ = try vm.save() }
        vm.privateKey = Data("KEY".utf8)
        #expect((try? vm.save()) != nil)
    }

    @Test("host and username are whitespace-trimmed on save")
    func whitespaceTrim() throws {
        let vm = ConnectionFormViewModel(mode: .add)
        vm.host = "  host.example.com  "
        vm.username = "  deploy  "
        vm.password = "p"
        _ = try vm.save()
        let saved = listEntries().first
        #expect(saved?.credential.host == "host.example.com")
        #expect(saved?.credential.username == "deploy")
    }

    @Test("canSave gates Save in add mode with secret required")
    func canSaveGate() {
        let vm = ConnectionFormViewModel(mode: .add)
        #expect(!vm.canSave)
        vm.host = "h"; vm.username = "u"
        #expect(!vm.canSave)
        vm.password = "p"
        #expect(vm.canSave)
    }
}
