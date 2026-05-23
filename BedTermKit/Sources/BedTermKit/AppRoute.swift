import Foundation

enum AppRoute: Hashable {
    /// Push the add/edit form. `nil` id means "add a new host"; a non-nil id
    /// means "edit the existing entry with this id" (resolved by the form screen
    /// from `HostsStore`).
    case hostForm(SavedHost.ID?)
    /// Per-host list of running + killed sessions
    /// (PRD bedterm/background-sessions R2).
    case sessionsPanel(SavedHost.ID)
    /// Frozen view of a killed session (PRD R4). Carries the
    /// `SessionSnapshot.id` so the destination resolves the snapshot
    /// from `PersistedSessionSnapshotStore`.
    case killedSessionDetail(UUID)
    case terminal
    #if DEBUG
        /// Loopback `bedterm-mock-ssh` server on 127.0.0.1:2222 with
        /// hard-coded credentials. Exercises the real Citadel SSH client
        /// + block view end-to-end without going through the Hosts list.
        case mockSSH
    #endif
}
