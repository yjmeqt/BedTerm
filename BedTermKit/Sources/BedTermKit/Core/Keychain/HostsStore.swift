import BedTermIOS
import Foundation
import os

/// Persists the user's saved-hosts list.
///
/// Storage lives in Rust (`rust-core/bedterm-ios/src/hosts_store.rs`) —
/// per-UUID Keychain blobs keyed under service
/// `com.applovin.yi.bedterm.savedHosts` plus a `UserDefaults`-backed
/// `hosts.order` index. This Swift wrapper round-trips the Codable
/// `SavedHost` shape through the `bt_ios_hosts_*` C ABI; the blob shape
/// stays opaque to Rust except when building the display snapshot
/// (`bt_ios_hosts_snapshot_json`), where Rust pulls a handful of fields
/// out via `serde_json::Value` for the hosts list cell.
///
/// Reconciliation: if the `UserDefaults` index falls behind the
/// Keychain (e.g. after a reinstall that preserved the Keychain but
/// reset UserDefaults), the Rust snapshot path appends orphans to the
/// order array and rewrites it — the user does not silently lose their
/// list. See PRD R5.reconcile_keychain_on_empty_index.
public struct HostsStore: Sendable {
    private static let log = Logger(subsystem: "com.applovin.yi.bedterm", category: "HostsStore")

    public init() {}

    // MARK: - Read

    /// Returns the saved hosts in display order. Pulls the snapshot
    /// header from Rust (id list + reconciliation), then loads each
    /// blob and decodes back to `SavedHost`. Entries that fail to
    /// decode are logged and skipped.
    public func list() -> [SavedHost] {
        guard let cStr = bt_ios_hosts_snapshot_json() else { return [] }
        defer { bt_ios_hosts_free_string(cStr) }
        let json = String(cString: cStr)
        guard let data = json.data(using: .utf8),
            let array = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else {
            return []
        }
        var out: [SavedHost] = []
        out.reserveCapacity(array.count)
        for entry in array {
            guard let idString = entry["id"] as? String,
                let uuid = UUID(uuidString: idString)
            else { continue }
            do {
                out.append(try self.load(id: uuid))
            } catch {
                let desc = String(describing: error)
                Self.log.error(
                    "Failed to load saved host \(idString, privacy: .public): \(desc, privacy: .public)"
                )
            }
        }
        return out
    }

    public func load(id: UUID) throws -> SavedHost {
        var len: UInt = 0
        let ptr = id.uuidString.withCString { idPtr in
            bt_ios_hosts_load_blob(idPtr, &len)
        }
        guard let ptr, len > 0 else {
            throw KeychainError.notFound
        }
        defer { bt_ios_hosts_free_blob(ptr, len) }
        let data = Data(bytes: ptr, count: Int(len))
        return try JSONDecoder().decode(SavedHost.self, from: data)
    }

    // MARK: - Write

    /// Saves an entry. Appends a new id to the end of the order on first save;
    /// preserves position on update.
    public func save(_ entry: SavedHost) throws {
        let data = try JSONEncoder().encode(entry)
        let ok = entry.id.uuidString.withCString { idPtr in
            data.withUnsafeBytes { raw -> Bool in
                let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                return bt_ios_hosts_save_blob(idPtr, base, UInt(raw.count))
            }
        }
        if !ok {
            throw KeychainError.status(0)
        }
    }

    public func delete(id: UUID) {
        id.uuidString.withCString { idPtr in
            bt_ios_hosts_delete(idPtr)
        }
    }
}
