import Foundation
import os

/// Persists the user's saved-hosts list.
///
/// Each entry's secret material lives in its own Keychain item, keyed by UUID under
/// `service`. Display order lives in `UserDefaults` because the Keychain has no
/// stable ordering of its own.
///
/// Reconciliation: if the `UserDefaults` index is empty but the Keychain still has
/// items (e.g. after a reinstall that preserved the Keychain or a UserDefaults
/// wipe), the store rebuilds the index from the surviving Keychain items so the
/// user does not silently lose their list. See PRD R5.reconcile_keychain_on_empty_index.
public struct HostsStore {
    public static let defaultService = "com.applovin.yi.bedterm.savedHosts"
    public static let defaultOrderKey = "hosts.order"
    public static let legacyMigrationKey = "hosts.legacyMigrationDone"

    private static let log = Logger(subsystem: "com.applovin.yi.bedterm", category: "HostsStore")

    private let service: String
    private let orderKey: String
    private let migrationKey: String
    private let defaults: UserDefaults
    private let legacy: CredentialsStore

    public init(
        service: String = HostsStore.defaultService,
        orderKey: String = HostsStore.defaultOrderKey,
        migrationKey: String = HostsStore.legacyMigrationKey,
        defaults: UserDefaults = .standard,
        legacy: CredentialsStore = CredentialsStore()
    ) {
        self.service = service
        self.orderKey = orderKey
        self.migrationKey = migrationKey
        self.defaults = defaults
        self.legacy = legacy
    }

    // MARK: - Read

    /// Returns the saved hosts in display order. Reconciles the index against the
    /// Keychain when one looks stale relative to the other.
    public func list() -> [SavedHost] {
        let storedIDs = self.storedOrder()
        let keychainIDs = Keychain.allAccounts(service: self.service)
        let keychainSet = Set(keychainIDs)
        let validOrdered = storedIDs.filter { keychainSet.contains($0.uuidString) }

        let validSet = Set(validOrdered.map(\.uuidString))
        let orphans =
            keychainIDs
            .filter { !validSet.contains($0) }
            .compactMap(UUID.init(uuidString:))
            .sorted { $0.uuidString < $1.uuidString }

        let resolvedOrder = validOrdered + orphans
        if resolvedOrder.map(\.uuidString) != storedIDs.map(\.uuidString) {
            self.writeOrder(resolvedOrder)
        }

        return resolvedOrder.compactMap { id in
            do {
                return try self.load(id: id)
            } catch {
                let desc = String(describing: error)
                Self.log.error(
                    "Failed to load saved host \(id.uuidString, privacy: .public): \(desc, privacy: .public)")
                return nil
            }
        }
    }

    public func load(id: UUID) throws -> SavedHost {
        let data = try Keychain.load(service: self.service, account: id.uuidString)
        return try JSONDecoder().decode(SavedHost.self, from: data)
    }

    // MARK: - Write

    /// Saves an entry. Appends a new id to the end of the order on first save;
    /// preserves position on update.
    public func save(_ entry: SavedHost) throws {
        let data = try JSONEncoder().encode(entry)
        try Keychain.save(service: self.service, account: entry.id.uuidString, data: data)
        var order = self.storedOrder()
        if !order.contains(entry.id) {
            order.append(entry.id)
            self.writeOrder(order)
        }
    }

    public func delete(id: UUID) {
        Keychain.delete(service: self.service, account: id.uuidString)
        var order = self.storedOrder()
        order.removeAll { $0 == id }
        self.writeOrder(order)
    }

    // MARK: - Migration

    /// Seeds the saved-hosts list from the legacy single-credential entry, if any.
    ///
    /// Step order matters for crash safety (PRD R6.migration_step_order):
    ///   1. Write new SavedHost to Keychain.
    ///   2. Append its id to the order index.
    ///   3. Delete the legacy item.
    /// If a crash strands us between (1) and (2), `list()` recovers the orphan;
    /// between (2) and (3), `migrationIsIdempotent` checks the existing match and
    /// skips creating a duplicate.
    @discardableResult
    public func migrateLegacyIfNeeded(label: String = String(localized: "Last connection")) -> Bool {
        if self.defaults.bool(forKey: self.migrationKey) {
            return false
        }
        let legacyCredential: HostCredential
        do {
            legacyCredential = try self.legacy.load()
        } catch KeychainError.notFound {
            self.defaults.set(true, forKey: self.migrationKey)
            return false
        } catch {
            // R6.migration_tolerates_corruption: log and proceed; user is not blocked.
            Self.log.error(
                "Legacy credential decode failed during migration: \(String(describing: error), privacy: .public)")
            self.defaults.set(true, forKey: self.migrationKey)
            return false
        }

        let current = self.list()
        let alreadyMigrated = current.contains { existing in
            existing.credential.host == legacyCredential.host
                && existing.credential.port == legacyCredential.port
                && existing.credential.username == legacyCredential.username
        }
        if alreadyMigrated {
            self.legacy.delete()
            self.defaults.set(true, forKey: self.migrationKey)
            return false
        }

        let entry = SavedHost(label: label, credential: legacyCredential)
        do {
            try self.save(entry)
        } catch {
            Self.log.error("Migration write failed: \(String(describing: error), privacy: .public)")
            return false
        }
        self.legacy.delete()
        self.defaults.set(true, forKey: self.migrationKey)
        return true
    }

    // MARK: - Internal

    private func storedOrder() -> [UUID] {
        let raw = self.defaults.array(forKey: self.orderKey) as? [String] ?? []
        return raw.compactMap(UUID.init(uuidString:))
    }

    private func writeOrder(_ order: [UUID]) {
        self.defaults.set(order.map(\.uuidString), forKey: self.orderKey)
    }
}
