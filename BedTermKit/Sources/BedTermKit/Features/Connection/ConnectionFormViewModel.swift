import BedTermIOS
import Foundation
import Observation

/// `@Observable` proxy over the Rust-owned connect-form state machine
/// (`connect_form_vm` / `bt_ios_connect_form_vm_*`). Every property write
/// forwards to Rust and mirrors the new state into the local
/// `@Observable` properties so existing `withObservationTracking`
/// observers keep firing on the same key paths.
///
/// `save()` asks Rust to validate + resolve every field (including
/// secret preservation in edit mode) via
/// `bt_ios_connect_form_vm_try_save`, decodes the resolved JSON, and
/// persists the `SavedHost` blob directly through the Rust Keychain
/// FFI (`bt_ios_hosts_save_blob`).
@MainActor
@Observable
public final class ConnectionFormViewModel {
    public enum Mode: Equatable {
        case add
        case edit(SavedHost)
    }

    public enum AuthChoice: Equatable { case password, privateKey }

    public enum FormError: Error, Equatable { case validation(String) }

    public let mode: Mode

    public var label: String = "" {
        didSet { self.pushString(self.label, bt_ios_connect_form_vm_set_label) }
    }
    public var host: String = "" {
        didSet { self.pushString(self.host, bt_ios_connect_form_vm_set_host) }
    }
    public var port: String = "22" {
        didSet { self.pushString(self.port, bt_ios_connect_form_vm_set_port_text) }
    }
    public var username: String = "" {
        didSet { self.pushString(self.username, bt_ios_connect_form_vm_set_username) }
    }
    public var password: String = "" {
        didSet {
            self.pushString(self.password, bt_ios_connect_form_vm_set_password)
            self.passwordTouched = true
        }
    }
    public var privateKey: Data? {
        didSet {
            if let data = self.privateKey {
                data.withUnsafeBytes { raw in
                    let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                    bt_ios_connect_form_vm_set_private_key_bytes(base, UInt(raw.count))
                }
            } else {
                bt_ios_connect_form_vm_set_private_key_bytes(nil, 0)
            }
            self.privateKeyTouched = true
        }
    }
    public var passphrase: String = "" {
        didSet {
            self.pushString(self.passphrase, bt_ios_connect_form_vm_set_passphrase)
            self.passphraseTouched = true
        }
    }
    public var auth: AuthChoice = .password {
        didSet { bt_ios_connect_form_vm_set_using_key(self.auth == .privateKey) }
    }

    /// True once the user types into the secret field; in edit mode an
    /// untouched secret means "preserve what's in the Keychain", a
    /// touched one means "overwrite". Mirrored from Rust's dirty
    /// tracking; setting these directly only affects the local mirror —
    /// Rust observes its own dirty flag from each setter.
    public var passwordTouched: Bool = false
    public var privateKeyTouched: Bool = false
    public var passphraseTouched: Bool = false

    public var errorMessage: String?

    public init(mode: Mode) {
        self.mode = mode
        switch mode {
        case .add:
            bt_ios_connect_form_vm_reset_to_add()
            self.port = "22"
        case .edit(let entry):
            self.pushEditPrefill(entry)
            self.label = entry.label
            self.host = entry.credential.host
            self.port = String(entry.credential.port)
            self.username = entry.credential.username
            switch entry.credential.auth {
            case .password:
                self.auth = .password
            case .privateKey:
                self.auth = .privateKey
            }
            // Resetting the touched-flag mirror after the property
            // sets above set them implicitly. Rust's prefill_edit
            // already cleared its own dirty flags.
            self.passwordTouched = false
            self.privateKeyTouched = false
            self.passphraseTouched = false
        }
    }

    // MARK: - Derived

    /// True when the form has enough content for Save to be enabled.
    public var canSave: Bool {
        bt_ios_connect_form_vm_can_save()
    }

    /// Looks up an existing saved entry that matches `(host, port, username)`
    /// other than the entry being edited. Drives the inline warning in
    /// R3.duplicate_host_warning.
    public func duplicate() -> SavedHost? {
        let normalized = self.normalizedHost()
        guard !normalized.isEmpty, let portValue = self.normalizedPort() else { return nil }
        let user = self.username.trimmingCharacters(in: .whitespaces)
        guard !user.isEmpty else { return nil }
        let excludeID: UUID? = {
            if case .edit(let entry) = self.mode { return entry.id }
            return nil
        }()
        guard let snapshotPtr = bt_ios_hosts_snapshot_json() else { return nil }
        defer { bt_ios_hosts_free_string(snapshotPtr) }
        let json = String(cString: snapshotPtr)
        guard let data = json.data(using: .utf8),
            let items = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return nil }
        for item in items {
            guard let idStr = item["id"] as? String,
                let uuid = UUID(uuidString: idStr),
                uuid != excludeID,
                let hostStr = item["host"] as? String, hostStr == normalized,
                let portVal = item["port"] as? Int, portVal == portValue,
                let userStr = item["username"] as? String, userStr == user
            else { continue }
            guard let entryPtr = uuid.uuidString.withCString({ bt_ios_hosts_load_json($0) }) else {
                continue
            }
            defer { bt_ios_hosts_free_string(entryPtr) }
            if let entryData = String(cString: entryPtr).data(using: .utf8),
                let entry = try? JSONDecoder().decode(SavedHost.self, from: entryData) {
                return entry
            }
        }
        return nil
    }

    // MARK: - Save

    /// Validates via the Rust VM, materialises a `SavedHost` from the
    /// resolved fields, and persists. Returns the entry's id so the
    /// caller can dismiss back to the list with the new row visible.
    @discardableResult
    public func save() throws -> SavedHost.ID {
        guard let outcomeJSON = bt_ios_connect_form_vm_try_save() else {
            // Validation failed — pull the error message from Rust.
            let messagePtr = bt_ios_connect_form_vm_error_message()
            let message: String
            if let messagePtr {
                message = String(cString: messagePtr)
                bt_ios_connect_form_vm_free_string(messagePtr)
            } else {
                message = String(localized: "Could not save host.")
            }
            self.errorMessage = message
            throw FormError.validation(message)
        }
        defer { bt_ios_connect_form_vm_free_string(outcomeJSON) }
        let json = String(cString: outcomeJSON)
        guard let entry = Self.decodeSaveOutcome(json, mode: self.mode) else {
            let message = String(localized: "Internal error decoding form data.")
            self.errorMessage = message
            throw FormError.validation(message)
        }
        let data: Data
        do {
            data = try JSONEncoder().encode(entry)
        } catch {
            let msg = String(localized: "Could not save host.")
            self.errorMessage = msg
            throw FormError.validation(msg)
        }
        let ok = entry.id.uuidString.withCString { idPtr in
            data.withUnsafeBytes { raw -> Bool in
                let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                return bt_ios_hosts_save_blob(idPtr, base, UInt(raw.count))
            }
        }
        if !ok {
            let msg = String(localized: "Could not save host.")
            self.errorMessage = msg
            throw FormError.validation(msg)
        }
        self.errorMessage = nil
        return entry.id
    }

    // MARK: - Helpers

    /// Splits a pasted `host:port` into the corresponding fields, then
    /// trims. Kept as a thin Swift-side mirror for `duplicate()`; the
    /// Rust VM applies the same rule on save.
    func normalizedHost() -> String {
        let raw = self.host.trimmingCharacters(in: .whitespaces)
        guard !raw.isEmpty else { return "" }

        if raw.hasPrefix("["), let bracket = raw.firstIndex(of: "]") {
            let after = raw.index(after: bracket)
            if after < raw.endIndex && raw[after] == ":" {
                return String(raw[..<bracket]).replacingOccurrences(of: "[", with: "")
            }
            return raw
        }

        let colonCount = raw.filter { $0 == ":" }.count
        guard colonCount == 1, let colon = raw.lastIndex(of: ":") else { return raw }
        guard Int(raw[raw.index(after: colon)...]) != nil else { return raw }
        return String(raw[..<colon])
    }

    func normalizedPort() -> Int? {
        let raw = self.host.trimmingCharacters(in: .whitespaces)
        if raw.hasPrefix("["), let bracket = raw.firstIndex(of: "]") {
            let after = raw.index(after: bracket)
            if after < raw.endIndex && raw[after] == ":" {
                let portPart = raw[raw.index(after: after)...]
                if let parsed = Int(portPart) { return parsed }
            }
        }
        let colonCount = raw.filter { $0 == ":" }.count
        if colonCount == 1, let colon = raw.lastIndex(of: ":") {
            if let parsed = Int(raw[raw.index(after: colon)...]) { return parsed }
        }
        return Int(self.port.trimmingCharacters(in: .whitespaces))
    }

    // MARK: - Bridge plumbing

    private func pushString(
        _ value: String,
        _ setter: (UnsafePointer<CChar>?) -> Void
    ) {
        value.withCString { setter($0) }
    }

    private func pushEditPrefill(_ entry: SavedHost) {
        let existingPassword: String?
        let existingKey: Data?
        let existingPass: String?
        let authIsKey: Bool
        switch entry.credential.auth {
        case .password(let pw):
            existingPassword = pw
            existingKey = nil
            existingPass = nil
            authIsKey = false
        case .privateKey(let key, let pass):
            existingPassword = nil
            existingKey = key
            existingPass = pass
            authIsKey = true
        }
        let id = entry.id.uuidString
        Self.callPrefill(
            id: id,
            label: entry.label,
            host: entry.credential.host,
            port: UInt16(entry.credential.port),
            username: entry.credential.username,
            authIsKey: authIsKey,
            existingPassword: existingPassword,
            existingKey: existingKey,
            existingPassphrase: existingPass
        )
    }

    // Nest the `withCString` / `withUnsafeBytes` borrows so all
    // pointers remain valid for the duration of the FFI call.
    // swiftlint:disable:next function_parameter_count
    private static func callPrefill(
        id: String,
        label: String,
        host: String,
        port: UInt16,
        username: String,
        authIsKey: Bool,
        existingPassword: String?,
        existingKey: Data?,
        existingPassphrase: String?
    ) {
        id.withCString { idPtr in
            label.withCString { labelPtr in
                host.withCString { hostPtr in
                    username.withCString { userPtr in
                        let withPassword: (UnsafePointer<CChar>?) -> Void = { pwPtr in
                            let withPass: (UnsafePointer<CChar>?) -> Void = { ppPtr in
                                let withKey: (UnsafePointer<UInt8>?, Int) -> Void = { keyPtr, keyLen in
                                    bt_ios_connect_form_vm_prefill_edit(
                                        idPtr, labelPtr, hostPtr, port, userPtr,
                                        authIsKey, pwPtr, keyPtr, UInt(keyLen), ppPtr
                                    )
                                }
                                if let key = existingKey {
                                    key.withUnsafeBytes { raw in
                                        let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                                        withKey(base, raw.count)
                                    }
                                } else {
                                    withKey(nil, 0)
                                }
                            }
                            if let pass = existingPassphrase {
                                pass.withCString { withPass($0) }
                            } else {
                                withPass(nil)
                            }
                        }
                        if let pwd = existingPassword {
                            pwd.withCString { withPassword($0) }
                        } else {
                            withPassword(nil)
                        }
                    }
                }
            }
        }
    }

    /// Decode the JSON `SaveOutcome` returned by Rust into a fully
    /// materialised `SavedHost`. Uses the original entry id when in
    /// edit mode (the Rust VM hands back the same id we passed in).
    private static func decodeSaveOutcome(_ json: String, mode: Mode) -> SavedHost? {
        guard let data = json.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        guard let idString = obj["id"] as? String,
            let uuid = UUID(uuidString: idString),
            let host = obj["host"] as? String,
            let port = obj["port"] as? Int,
            let username = obj["username"] as? String,
            let authIsKey = obj["authIsKey"] as? Bool
        else { return nil }
        let label = (obj["label"] as? String) ?? ""
        let auth: HostCredential.AuthMethod
        if authIsKey {
            guard let keyB64 = obj["privateKeyBase64"] as? String,
                let keyData = Data(base64Encoded: keyB64)
            else { return nil }
            let pass = obj["passphrase"] as? String
            auth = .privateKey(keyData, passphrase: pass)
        } else {
            let pw = (obj["password"] as? String) ?? ""
            auth = .password(pw)
        }
        let credential = HostCredential(host: host, port: port, username: username, auth: auth)
        // In edit mode preserve the existing UUID (Rust hands the same
        // string back); in add mode use the freshly-minted one from
        // Rust.
        let finalID: UUID = {
            if case .edit(let existing) = mode {
                return existing.id
            }
            return uuid
        }()
        return SavedHost(id: finalID, label: label, credential: credential)
    }
}

/// Result reported by the connect-form flow back to the hosts screen.
public enum ConnectionFormOutcome {
    /// User saved the entry. The host's id is provided so the caller
    /// can scroll it into view on the list.
    case saved(SavedHost.ID)
    /// User saved the entry AND wants to start a connect to it
    /// immediately (the first-run shortcut path).
    case savedAndConnect(SavedHost.ID)
    /// User cancelled the form.
    case cancelled
}
