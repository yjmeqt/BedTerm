import Foundation
import Observation
import UIKit

/// Drives the saved-hosts list: holds the entry array, per-row in-flight state,
/// and the connect flow that fans out to `ConnectAttempt`. Errors,
/// session-end, device-locked, and host-key-mismatch events surface through the
/// injected `Toaster` rather than per-row banners.
@MainActor
@Observable
public final class HostsViewModel {
    public struct PendingMismatch: Equatable {
        public let stored: String
        public let remote: String
        public let host: String
        public let port: Int
        public let sourceID: UUID
    }

    public struct SwapConfirmation: Equatable, Identifiable {
        public let targetID: UUID
        public let displayName: String
        public var id: UUID { self.targetID }
    }

    public struct DeleteConfirmation: Equatable, Identifiable {
        public let targetID: UUID
        public let displayName: String
        public let isLive: Bool
        public let isInFlight: Bool
        public var id: UUID { self.targetID }
    }

    public private(set) var entries: [SavedHost] = []
    public private(set) var inFlightID: UUID?
    private(set) var lastSession: TerminalSession?
    public private(set) var currentSessionID: UUID?
    public var pendingMismatch: PendingMismatch?
    public var swapConfirmation: SwapConfirmation?
    public var deleteConfirmation: DeleteConfirmation?
    public private(set) var loadFailed: Bool = false
    public private(set) var didMigrate: Bool = false

    /// Surfaced to the view layer so it can fire toasts. The closure is invoked
    /// on the main actor for every connect outcome (success cases just hint
    /// success / "session ready"; error cases carry message + flags so the view
    /// can render a Retry/Open-Settings toast). Set by `HostsScreen` on first
    /// connect dispatch; tests can leave it nil.
    public var onConnectError: ((UUID, String, Bool) -> Void)?
    /// Resolves the shell-integration heredoc to push at connect time.
    /// Set by `HostsScreen` after the SwiftUI environment is wired so we
    /// can read `BedTermSettings.installShellIntegrationOnConnect`. Nil
    /// keeps the channel pristine.
    public var bootstrapPayloadProvider: (@MainActor () -> String?)?

    private let store: HostsStore
    private let connectFactory: @MainActor () -> ConnectAttempt
    private var inFlightTask: Task<Void, Never>?
    /// Initial input to send after the SSH channel opens — used to
    /// implement "Resume here" (PRD R4.detail_resume_action), which
    /// queues `cd <quoted-cwd>\n` to land the new shell in the killed
    /// session's last CWD. Cleared as soon as it's been sent.
    private var pendingInitialInput: [UUID: String] = [:]
    /// Killed-session snapshot store. Set after init by the host screen
    /// from the SwiftUI environment so this view model can preserve
    /// scrollback + last-CWD when a session ends (PRD R1.killed_keeps_snapshot).
    public weak var snapshotStore: SessionSnapshotStore?

    public init(store: HostsStore = HostsStore()) {
        self.store = store
        self.connectFactory = { ConnectAttempt(clientFactory: { CitadelSSHClient() }) }
    }

    init(store: HostsStore, connectFactory: @MainActor @escaping () -> ConnectAttempt) {
        self.store = store
        self.connectFactory = connectFactory
    }

    // MARK: - Loading

    public func load() {
        if !UIApplication.shared.isProtectedDataAvailable {
            self.loadFailed = true
            return
        }
        self.didMigrate = self.store.migrateLegacyIfNeeded()
        self.entries = self.store.list()
        self.loadFailed = false
    }

    public func retryLoad() {
        self.load()
    }

    /// Entry point bound to the row's Connect button. Performs the swap-confirm
    /// dance if another session is live; otherwise hands straight to `connect`.
    public func requestConnect(id: UUID) {
        if self.inFlightID == id { return }
        if let sessionID = self.currentSessionID, sessionID != id {
            self.swapConfirmation = SwapConfirmation(
                targetID: id, displayName: self.displayName(for: id)
            )
            return
        }
        self.connect(id: id)
    }

    public func confirmSwap() {
        guard let target = self.swapConfirmation?.targetID else { return }
        self.swapConfirmation = nil
        self.lastSession?.disconnect()
        self.lastSession = nil
        self.currentSessionID = nil
        self.connect(id: target)
    }

    public func cancelSwap() { self.swapConfirmation = nil }

    // MARK: - Connect

    public func connect(id: UUID) {
        // R2.rapid_switch_cancels: cancel any prior attempt silently.
        if let prior = self.inFlightID, prior != id {
            self.inFlightTask?.cancel()
            if self.inFlightID == prior { self.inFlightID = nil }
        }
        self.inFlightID = id

        let task = Task { @MainActor in
            await self.runConnect(id: id)
        }
        self.inFlightTask = task
    }

    private func runConnect(id: UUID) async {
        let entry: SavedHost
        do {
            entry = try self.store.load(id: id)
        } catch {
            self.onConnectError?(
                id,
                String(localized: "Could not load saved host."),
                false
            )
            if self.inFlightID == id { self.inFlightID = nil }
            return
        }
        let attempt = self.connectFactory()
        let outcome = await attempt.run(
            credential: entry.credential,
            bootstrapPayload: self.bootstrapPayloadProvider?())
        if Task.isCancelled { return }

        switch outcome {
        case .session(let session):
            self.lastSession = session
            self.currentSessionID = id
            if let initialCommand = self.pendingInitialInput.removeValue(forKey: id) {
                // Brief delay so the bootstrap heredoc (if any) finishes
                // sourcing and the next prompt is drawn before we queue
                // the resume `cd` into the PTY.
                Task { [weak session] in
                    try? await Task.sleep(for: .milliseconds(400))
                    guard let session else { return }
                    await MainActor.run {
                        session.send(Data(initialCommand.utf8))
                    }
                }
            }
        case .mismatch(let stored, let remote, let host, let port):
            self.pendingMismatch = PendingMismatch(
                stored: stored, remote: remote, host: host, port: port, sourceID: id
            )
        case .error(let message, let permissionDenied):
            self.onConnectError?(id, message, permissionDenied)
        }
        if self.inFlightID == id { self.inFlightID = nil }
    }

    /// Called after the user trusts a new host key on the mismatch sheet.
    public func retryAfterMismatch() {
        guard let mismatch = self.pendingMismatch else { return }
        let id = mismatch.sourceID
        self.pendingMismatch = nil
        self.connect(id: id)
    }

    public func clearMismatch() {
        self.pendingMismatch = nil
    }

    /// Called when the terminal screen tears down so the next row tap starts fresh.
    public func sessionEnded() {
        self.lastSession = nil
        self.currentSessionID = nil
    }

    /// Snapshot the current session into the killed-session store and
    /// tear it down (PRD R1.killed_keeps_snapshot, R3.kill_button).
    /// Safe to call even when there is no live session — it just falls
    /// through to `sessionEnded()`.
    public func snapshotAndEnd(reason: SessionSnapshot.KillReason) {
        guard
            let session = self.lastSession,
            let hostID = self.currentSessionID,
            let snapshotStore = self.snapshotStore
        else {
            self.sessionEnded()
            return
        }
        let blocks = session.blockStore.blocks
        let lastCwd = session.blockStore.latestPwd
        let lastCompleted = blocks.last(where: { !$0.isRunning && !$0.command.isEmpty })
        let snapshot = SessionSnapshot(
            hostID: hostID,
            blocks: blocks,
            lastCwd: lastCwd,
            lastCommand: lastCompleted?.command,
            lastExitCode: lastCompleted?.exitCode,
            killReason: reason
        )
        snapshotStore.record(snapshot)
        session.disconnect()
        self.lastSession = nil
        self.currentSessionID = nil
    }

    /// Re-launch a fresh session for the given host with an optional
    /// initial command to send after the channel opens (PRD R4.detail_resume_action).
    /// Used by `KilledSessionDetailScreen` to land the new shell in
    /// the killed session's last CWD via a queued `cd`.
    public func requestResume(hostID: UUID, initialCommand: String?) {
        if let cmd = initialCommand, !cmd.isEmpty {
            self.pendingInitialInput[hostID] = cmd
        }
        self.requestConnect(id: hostID)
    }

    // MARK: - Delete

    public func requestDelete(id: UUID) {
        let isLive = self.currentSessionID == id
        let isInFlight = self.inFlightID == id
        self.deleteConfirmation = DeleteConfirmation(
            targetID: id,
            displayName: self.displayName(for: id),
            isLive: isLive,
            isInFlight: isInFlight
        )
    }

    public func confirmDelete() {
        guard let conf = self.deleteConfirmation else { return }
        self.deleteConfirmation = nil
        if conf.isInFlight {
            self.inFlightTask?.cancel()
            if self.inFlightID == conf.targetID { self.inFlightID = nil }
        }
        if conf.isLive {
            self.lastSession?.disconnect()
            self.lastSession = nil
            self.currentSessionID = nil
        }
        self.store.delete(id: conf.targetID)
        self.entries.removeAll { $0.id == conf.targetID }
    }

    public func cancelDelete() { self.deleteConfirmation = nil }

    // MARK: - Display helpers

    public func displayName(for id: UUID) -> String {
        guard let entry = self.entries.first(where: { $0.id == id }) else { return "" }
        if !entry.label.isEmpty { return entry.label }
        return "\(entry.credential.username)@\(entry.credential.host)"
    }

    /// Reload after a form save so a freshly-added entry appears.
    public func refresh() {
        self.entries = self.store.list()
    }
}
