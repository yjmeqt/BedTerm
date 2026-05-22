import Foundation
import Observation

/// In-memory ring of killed-session snapshots, scoped per host.
/// Lifetime: process — `SessionSnapshot.snapshotsInMemoryOnlyMvp`
/// (PRD R7.snapshots_in_memory_only_mvp).
@MainActor
@Observable
public final class SessionSnapshotStore {
    /// Per-host cap (PRD R7.killed_snapshot_cap, target 10).
    public static let perHostCap = 10

    public private(set) var snapshots: [SessionSnapshot] = []

    public init() {}

    /// Append a killed-session snapshot, enforcing FIFO eviction per host.
    public func record(_ snapshot: SessionSnapshot) {
        snapshots.insert(snapshot, at: 0)
        let hostSnapshots = snapshots.filter { $0.hostID == snapshot.hostID }
        guard hostSnapshots.count > Self.perHostCap, let oldest = hostSnapshots.last,
            let dropIndex = snapshots.firstIndex(where: { $0.id == oldest.id })
        else { return }
        snapshots.remove(at: dropIndex)
    }

    public func snapshots(forHost hostID: UUID) -> [SessionSnapshot] {
        snapshots.filter { $0.hostID == hostID }
    }

    public func snapshot(id: UUID) -> SessionSnapshot? {
        snapshots.first { $0.id == id }
    }

    public func discard(id: UUID) {
        snapshots.removeAll { $0.id == id }
    }

    public func discardAll(forHost hostID: UUID) {
        snapshots.removeAll { $0.hostID == hostID }
    }
}
