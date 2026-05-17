import Foundation
import Observation

/// Form state for adding or editing a saved host.
///
/// Owns nothing related to SSH or prewarming — those live in `ConnectAttempt` and
/// run from `HostsViewModel.connect(id:)` after the form is dismissed.
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

    public var label: String = ""
    public var host: String = ""
    public var port: String = "22"
    public var username: String = ""
    public var password: String = ""
    public var privateKey: Data?
    public var passphrase: String = ""
    public var auth: AuthChoice = .password

    /// True once the user types into the secret field; in edit mode an untouched
    /// secret means "preserve what's in the Keychain", a touched one means "overwrite".
    public var passwordTouched: Bool = false
    public var privateKeyTouched: Bool = false
    public var passphraseTouched: Bool = false

    public var errorMessage: String?

    private let originalAuth: AuthChoice
    private let store: HostsStore

    public init(mode: Mode, store: HostsStore = HostsStore()) {
        self.mode = mode
        self.store = store
        switch mode {
        case .add:
            self.originalAuth = .password
        case .edit(let entry):
            self.label = entry.label
            self.host = entry.credential.host
            self.port = String(entry.credential.port)
            self.username = entry.credential.username
            switch entry.credential.auth {
            case .password:
                self.auth = .password
                self.originalAuth = .password
            case .privateKey:
                self.auth = .privateKey
                self.originalAuth = .privateKey
            }
        }
    }

    // MARK: - Derived

    /// True when the form has enough content for Save to be enabled. Mirrors validation
    /// in `save()` so the Save button greys out before the user hits it.
    public var canSave: Bool {
        guard !self.normalizedHost().isEmpty,
            !self.username.trimmingCharacters(in: .whitespaces).isEmpty
        else { return false }
        guard let portValue = self.normalizedPort(), (1...65_535).contains(portValue)
        else { return false }
        switch self.auth {
        case .password:
            if self.requiresFreshSecret() {
                return !self.password.isEmpty
            }
            return true
        case .privateKey:
            if self.requiresFreshSecret() {
                return self.privateKey != nil
            }
            return true
        }
    }

    /// Looks up an existing saved entry that matches `(host, port, username)` other
    /// than the entry being edited. Returns its label (or empty string) when a dup
    /// exists, nil otherwise. Drives the inline warning in R3.duplicate_host_warning.
    public func duplicate() -> SavedHost? {
        let normalizedHost = self.normalizedHost()
        guard !normalizedHost.isEmpty,
            let portValue = self.normalizedPort()
        else { return nil }
        let user = self.username.trimmingCharacters(in: .whitespaces)
        guard !user.isEmpty else { return nil }
        let excludeID: UUID? = {
            if case .edit(let entry) = self.mode { return entry.id }
            return nil
        }()
        return self.store.list().first { entry in
            entry.id != excludeID
                && entry.credential.host == normalizedHost
                && entry.credential.port == portValue
                && entry.credential.username == user
        }
    }

    // MARK: - Save

    /// Validates, builds a `HostCredential`, and persists. Returns the entry's id
    /// so the caller (form screen) can dismiss back to the list with the new row
    /// visible / scrolled into view.
    @discardableResult
    public func save() throws -> SavedHost.ID {
        let credential = try self.buildCredential()
        let entry = self.buildEntry(credential: credential)
        do {
            try self.store.save(entry)
        } catch {
            let msg = String(localized: "Could not save host.")
            self.errorMessage = msg
            throw FormError.validation(msg)
        }
        self.errorMessage = nil
        return entry.id
    }

    private func buildCredential() throws -> HostCredential {
        let normalizedHost = self.normalizedHost()
        let user = self.username.trimmingCharacters(in: .whitespaces)
        guard !normalizedHost.isEmpty, !user.isEmpty else {
            let msg = String(localized: "Host, port and username are required.")
            self.errorMessage = msg
            throw FormError.validation(msg)
        }
        guard let portValue = self.normalizedPort(), (1...65_535).contains(portValue) else {
            let msg = String(localized: "Port must be between 1 and 65535.")
            self.errorMessage = msg
            throw FormError.validation(msg)
        }
        let authMethod = try self.buildAuthMethod()
        return HostCredential(host: normalizedHost, port: portValue, username: user, auth: authMethod)
    }

    private func buildAuthMethod() throws -> HostCredential.AuthMethod {
        switch self.auth {
        case .password:
            return self.preservedPasswordAuth() ?? .password(self.password)
        case .privateKey:
            if let preserved = self.preservedKeyAuth() { return preserved }
            guard let key = self.privateKey else {
                let msg = String(localized: "Please import a private key file.")
                self.errorMessage = msg
                throw FormError.validation(msg)
            }
            let pass = self.passphrase.isEmpty ? nil : self.passphrase
            return .privateKey(key, passphrase: pass)
        }
    }

    private func buildEntry(credential: HostCredential) -> SavedHost {
        let trimmedLabel = self.label.trimmingCharacters(in: .whitespacesAndNewlines)
        switch self.mode {
        case .add:
            return SavedHost(label: trimmedLabel, credential: credential)
        case .edit(let existing):
            var updated = existing
            updated.label = trimmedLabel
            updated.credential = credential
            return updated
        }
    }

    // MARK: - Helpers

    /// Splits a pasted `host:port` into the corresponding fields, then trims.
    /// Only splits when exactly one ':' is present (so unbracketed IPv6 literals
    /// are left alone) or when the input is `[ipv6]:port`.
    func normalizedHost() -> String {
        let raw = self.host.trimmingCharacters(in: .whitespaces)
        guard !raw.isEmpty else { return "" }

        if raw.hasPrefix("["), let bracket = raw.firstIndex(of: "]") {
            // `[ipv6]` or `[ipv6]:port`
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

    /// Port to persist — pulled out of either the Port field or the trailing
    /// `:port` portion of a pasted Host value.
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

    /// In edit mode, an untouched secret field of the same auth method means
    /// "leave the existing Keychain secret alone". A method switch always
    /// invalidates the prior secret.
    private func requiresFreshSecret() -> Bool {
        if case .add = self.mode { return true }
        if self.auth != self.originalAuth { return true }
        switch self.auth {
        case .password: return self.passwordTouched
        case .privateKey: return self.privateKeyTouched
        }
    }

    private func preservedPasswordAuth() -> HostCredential.AuthMethod? {
        guard case .edit(let entry) = self.mode,
            self.originalAuth == .password,
            !self.passwordTouched,
            case .password(let stored) = entry.credential.auth
        else { return nil }
        return .password(stored)
    }

    private func preservedKeyAuth() -> HostCredential.AuthMethod? {
        guard case .edit(let entry) = self.mode,
            self.originalAuth == .privateKey,
            !self.privateKeyTouched,
            case .privateKey(let storedKey, let storedPass) = entry.credential.auth
        else { return nil }
        let pass: String?
        if self.passphraseTouched {
            pass = self.passphrase.isEmpty ? nil : self.passphrase
        } else {
            pass = storedPass
        }
        return .privateKey(storedKey, passphrase: pass)
    }
}
