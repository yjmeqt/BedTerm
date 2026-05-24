import BedTermCoreC
import Foundation
import Observation

/// Rust-backed snapshot store. Reads killed-session metadata from SQLite
/// via the `PersistenceHandle` FFI layer.
@MainActor
@Observable
public final class PersistedSessionSnapshotStore {
    public private(set) var snapshots: [SessionSnapshot] = []

    private var handle: PersistenceHandle?

    public init(handle: PersistenceHandle?) {
        self.handle = handle
    }

    /// Upgrade a no-op store (constructed with `handle: nil`) to a
    /// SQLite-backed one once `PersistenceHandle.open` completes off-main.
    /// Lets BedTermApp hold a single store identity for the app lifetime
    /// so child views' captured references stay valid.
    public func attach(handle: PersistenceHandle) {
        if let existing = self.handle {
            assert(
                existing === handle,
                "PersistedSessionSnapshotStore.attach called twice with a different handle"
            )
            return
        }
        self.handle = handle
    }

    /// Pure filter over the cached snapshot list. Safe to call from SwiftUI
    /// `body`. Trigger a refresh explicitly via `reload(forHost:)` from a
    /// `.task` or `.onChange` — calling reload here mutates `snapshots` and
    /// invalidates the surrounding view, which spins into a render loop on
    /// devices that don't coalesce same-tick invalidations.
    public func snapshots(forHost hostID: UUID) -> [SessionSnapshot] {
        snapshots.filter { $0.hostID == hostID }
    }

    public func snapshot(id: UUID) -> SessionSnapshot? {
        snapshots.first { $0.id == id }
    }

    /// Reload snapshot metadata for one host from SQLite.
    public func reload(forHost hostID: UUID) {
        guard let handle else { return }
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
        guard let handle else {
            snapshots.removeAll { $0.id == id }
            return
        }
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
        // -1 (NULL in SQLite) happens for rows that escaped the Rust-side
        // orphan sweep — treat as AppRelaunch rather than UserKilled so the
        // detail screen doesn't claim the user ended a session they didn't.
        default: return .appRelaunch
        }
    }
}
