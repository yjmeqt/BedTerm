import BedTermCoreC
import Foundation
import Observation

/// Rust-backed snapshot store. Mirrors the in-memory `SessionSnapshotStore`
/// API so existing UI consumers can swap without code changes.
@MainActor
@Observable
public final class PersistedSessionSnapshotStore {
    public private(set) var snapshots: [SessionSnapshot] = []

    private let handle: PersistenceHandle

    public init(handle: PersistenceHandle) {
        self.handle = handle
    }

    /// Return all snapshots for a host, reloading from SQLite first. Cheap
    /// (returns at most 10 rows per host).
    public func snapshots(forHost hostID: UUID) -> [SessionSnapshot] {
        reload(forHost: hostID)
        return snapshots.filter { $0.hostID == hostID }
    }

    public func snapshot(id: UUID) -> SessionSnapshot? {
        snapshots.first { $0.id == id }
    }

    /// Reload snapshot metadata for one host from SQLite.
    public func reload(forHost hostID: UUID) {
        let hostStr = hostID.uuidString
        let listPtr: UnsafeMutablePointer<CSnapshotList>? = hostStr.withCString { cstr in
            bedterm_persistence_list(handle.unsafeHandle, cstr)
        }
        guard let listPtr else { return }
        defer { bedterm_persistence_free_list(listPtr) }

        let list = listPtr.pointee
        var rebuilt = snapshots.filter { $0.hostID != hostID }
        guard let items = list.items else {
            snapshots = rebuilt
            return
        }
        for idx in 0..<list.count {
            let item = items.advanced(by: Int(idx)).pointee
            guard let idCStr = item.id else { continue }
            guard let snapID = UUID(uuidString: String(cString: idCStr)) else { continue }
            let snap = SessionSnapshot(
                id: snapID,
                hostID: hostID,
                blocks: [],  // metadata-only; bytes only via open_replay (Task 5.x)
                lastCwd: item.last_cwd.map { String(cString: $0) },
                lastCommand: item.last_command.map { String(cString: $0) },
                lastExitCode: item.last_exit_code == Int32.min ? nil : item.last_exit_code,
                killedAt: Date(timeIntervalSince1970: item.killed_at),
                killReason: Self.killReason(rawValue: item.kill_reason)
            )
            rebuilt.append(snap)
        }
        // C side already orders by killed_at DESC; just assign.
        snapshots = rebuilt
    }

    public func discard(id: UUID) {
        id.uuidString.withCString { cstr in
            bedterm_persistence_discard(handle.unsafeHandle, cstr)
        }
        snapshots.removeAll { $0.id == id }
    }

    public func discardAll(forHost hostID: UUID) {
        let toDelete = snapshots.filter { $0.hostID == hostID }.map(\.id)
        for id in toDelete { discard(id: id) }
    }

    private static func killReason(rawValue: Int32) -> SessionSnapshot.KillReason {
        switch rawValue {
        case 0: return .userKilled
        case 1: return .remoteLogout
        case 2: return .networkDrop
        case 3: return .appRelaunch
        case 4: return .swapEvicted
        default: return .userKilled
        }
    }
}
