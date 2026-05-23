import Foundation
import Testing

@testable import BedTermKit

@MainActor
@Suite("ConnectionFormViewModel", .serialized)
struct ConnectionFormViewModelTests {
    private let service = "com.applovin.yi.bedterm.tests.formvm"
    private let orderKey: String
    private let migrationKey: String
    private let defaults: UserDefaults

    init() {
        TestKeychain.installInMemory()
        let suite = "BedTermTests.FormVM." + UUID().uuidString
        defaults = UserDefaults(suiteName: suite) ?? .standard
        defaults.removePersistentDomain(forName: suite)
        orderKey = "tests.formvm.order"
        migrationKey = "tests.formvm.migrationDone"
    }

    private func makeStore() -> HostsStore {
        HostsStore(
            service: service,
            orderKey: orderKey,
            migrationKey: migrationKey,
            defaults: defaults,
            legacy: CredentialsStore(service: service + ".legacy")
        )
    }

    @Test("save in add mode appends a new entry with all fields")
    func saveAddMode() throws {
        let store = makeStore()
        let vm = ConnectionFormViewModel(mode: .add, store: store)
        vm.label = "prod"
        vm.host = "10.0.0.5"
        vm.port = "22"
        vm.username = "deploy"
        vm.password = "hunter2"

        let id = try vm.save()
        let listed = store.list()
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
        let vm = ConnectionFormViewModel(mode: .add, store: makeStore())
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
        let vm = ConnectionFormViewModel(mode: .add, store: makeStore())
        vm.host = "h"; vm.username = "u"; vm.auth = .privateKey
        #expect(throws: ConnectionFormViewModel.FormError.self) { _ = try vm.save() }
        vm.privateKey = Data("KEY".utf8)
        #expect((try? vm.save()) != nil)
    }

    @Test("pasted host:port splits into Host and Port on save")
    func hostPasteSplit() throws {
        let store = makeStore()
        let vm = ConnectionFormViewModel(mode: .add, store: store)
        vm.host = "bastion.internal:2222"
        vm.username = "ops"
        vm.password = "p"
        _ = try vm.save()
        #expect(store.list().first?.credential.host == "bastion.internal")
        #expect(store.list().first?.credential.port == 2222)
    }

    @Test("bracketed IPv6 with port splits cleanly")
    func ipv6BracketedSplit() throws {
        let store = makeStore()
        let vm = ConnectionFormViewModel(mode: .add, store: store)
        vm.host = "[::1]:2222"
        vm.username = "ops"
        vm.password = "p"
        _ = try vm.save()
        #expect(store.list().first?.credential.host == "::1")
        #expect(store.list().first?.credential.port == 2222)
    }

    @Test("unbracketed IPv6 with multiple colons is preserved verbatim")
    func ipv6UnbracketedNotSplit() throws {
        let store = makeStore()
        let vm = ConnectionFormViewModel(mode: .add, store: store)
        vm.host = "fe80::1%en0"
        vm.username = "ops"
        vm.password = "p"
        vm.port = "22"
        _ = try vm.save()
        #expect(store.list().first?.credential.host == "fe80::1%en0")
    }

    @Test("host and username are whitespace-trimmed on save")
    func whitespaceTrim() throws {
        let store = makeStore()
        let vm = ConnectionFormViewModel(mode: .add, store: store)
        vm.host = "  host.example.com  "
        vm.username = "  deploy  "
        vm.password = "p"
        _ = try vm.save()
        let saved = store.list().first
        #expect(saved?.credential.host == "host.example.com")
        #expect(saved?.credential.username == "deploy")
    }

    @Test("edit mode preserves stored password when secret field is untouched")
    func editPreservesSecret() throws {
        let store = makeStore()
        let existing = SavedHost(
            label: "prod",
            credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("kept"))
        )
        try store.save(existing)

        let vm = ConnectionFormViewModel(mode: .edit(existing), store: store)
        vm.label = "prod-renamed"
        // No touch to password field.
        _ = try vm.save()
        let saved = try store.load(id: existing.id)
        #expect(saved.credential.auth == .password("kept"))
        #expect(saved.label == "prod-renamed")
    }

    @Test("edit mode overwrites stored password when user types a new value")
    func editOverwritesSecret() throws {
        let store = makeStore()
        let existing = SavedHost(
            label: "prod",
            credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("old"))
        )
        try store.save(existing)

        let vm = ConnectionFormViewModel(mode: .edit(existing), store: store)
        vm.password = "new"
        vm.passwordTouched = true
        _ = try vm.save()
        let saved = try store.load(id: existing.id)
        #expect(saved.credential.auth == .password("new"))
    }

    @Test("edit mode auth-method switch requires fresh secret")
    func authSwitchInvalidatesSecret() throws {
        let store = makeStore()
        let existing = SavedHost(
            label: "prod",
            credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("kept"))
        )
        try store.save(existing)

        let vm = ConnectionFormViewModel(mode: .edit(existing), store: store)
        vm.auth = .privateKey
        // No key imported yet — must fail.
        #expect(throws: ConnectionFormViewModel.FormError.self) { _ = try vm.save() }

        vm.privateKey = Data("K".utf8)
        vm.privateKeyTouched = true
        _ = try vm.save()
        let saved = try store.load(id: existing.id)
        if case .privateKey(let key, _) = saved.credential.auth {
            #expect(key == Data("K".utf8))
        } else {
            Issue.record("Expected privateKey auth after switch")
        }
    }

    @Test("duplicate() returns existing entry with same host/port/username")
    func duplicateDetection() throws {
        let store = makeStore()
        let existing = SavedHost(
            label: "prod",
            credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("p"))
        )
        try store.save(existing)

        let vm = ConnectionFormViewModel(mode: .add, store: store)
        vm.host = "h"; vm.username = "u"; vm.port = "22"; vm.password = "p"
        #expect(vm.duplicate()?.id == existing.id)
    }

    @Test("duplicate() excludes the entry being edited")
    func duplicateExcludesSelf() throws {
        let store = makeStore()
        let existing = SavedHost(
            label: "prod",
            credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("p"))
        )
        try store.save(existing)

        let vm = ConnectionFormViewModel(mode: .edit(existing), store: store)
        #expect(vm.duplicate() == nil)
    }

    @Test("canSave gates Save in add mode with secret required")
    func canSaveGate() {
        let vm = ConnectionFormViewModel(mode: .add, store: makeStore())
        #expect(!vm.canSave)
        vm.host = "h"; vm.username = "u"
        #expect(!vm.canSave)  // password still empty
        vm.password = "p"
        #expect(vm.canSave)
    }
}
