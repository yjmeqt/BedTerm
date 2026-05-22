import Foundation

/// Frozen record of a session that ended. Survives the SSH channel /
/// `TerminalSession` it came from so the sessions panel can list it and
/// the killed-session detail view can render its command history.
///
/// Output rows are *not* snapshotted in v1 — only block metadata
/// (command, exit code, duration, cwd, branch). Preserving the rendered
/// grid requires the `TerminalCore` strong-ownership refactor (P3 in
/// `BedTerm/docs/specs/background-sessions.md`).
public struct SessionSnapshot: Identifiable, Sendable, Equatable {
    public enum KillReason: String, Sendable, Equatable {
        case userKilled
        case remoteLogout
        case networkDrop
        case appRelaunch
        case swapEvicted
    }

    public let id: UUID
    public let hostID: UUID
    public let blocks: [Block]
    public let lastCwd: String?
    public let lastCommand: String?
    public let lastExitCode: Int32?
    public let killedAt: Date
    public let killReason: KillReason

    public init(
        id: UUID = UUID(),
        hostID: UUID,
        blocks: [Block],
        lastCwd: String?,
        lastCommand: String?,
        lastExitCode: Int32?,
        killedAt: Date = .now,
        killReason: KillReason
    ) {
        self.id = id
        self.hostID = hostID
        self.blocks = blocks
        self.lastCwd = lastCwd
        self.lastCommand = lastCommand
        self.lastExitCode = lastExitCode
        self.killedAt = killedAt
        self.killReason = killReason
    }
}

extension SessionSnapshot.KillReason {
    /// Human-readable, localized phrase for the killed-session detail
    /// header (PRD R4.detail_header_metadata).
    public var localizedReason: String {
        switch self {
        case .userKilled:
            return String(localized: "you ended this session")
        case .remoteLogout:
            return String(localized: "server closed the connection")
        case .networkDrop:
            return String(localized: "network drop")
        case .appRelaunch:
            return String(localized: "the app restarted")
        case .swapEvicted:
            return String(localized: "replaced by a newer session")
        }
    }
}
