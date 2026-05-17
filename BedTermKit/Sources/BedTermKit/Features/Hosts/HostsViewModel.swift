import Foundation
import Observation
import UIKit

/// Drives the saved-hosts list: holds the entry array, per-row in-flight /
/// error state, and the connect flow that fans out to `ConnectAttempt`.
@MainActor
@Observable
public final class HostsViewModel {
    public struct RowError: Equatable {
        public var message: String
        public var permissionDenied: Bool
        public var expanded: Bool
    }

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
    public var errorByID: [UUID: RowError] = [:]
    public var swapConfirmation: SwapConfirmation?
    public var deleteConfirmation: DeleteConfirmation?
    public private(set) var loadFailed: Bool = false
    public private(set) var didMigrate: Bool = false

    private let store: HostsStore
    private let connectFactory: @MainActor () -> ConnectAttempt
    private var inFlightTask: Task<Void, Never>?

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

    // MARK: - Row tap

    /// Tapping the row body only toggles error expansion. The Hosts list is
    /// pure display + storage; connection only starts via the explicit Connect
    /// button (routed through `requestConnect(id:)`).
    public func onRowBodyTap(id: UUID) {
        if self.errorByID[id] != nil {
            self.toggleErrorExpansion(id: id)
            return
        }
        self.collapseAllErrors()
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
        // R2.rapid_switch_cancels: cancel any prior attempt; the cancelled row
        // is silent (no error banner).
        if let prior = self.inFlightID, prior != id {
            self.inFlightTask?.cancel()
            self.errorByID[prior] = nil
            if self.inFlightID == prior { self.inFlightID = nil }
        }
        self.inFlightID = id
        self.errorByID[id] = nil

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
            self.errorByID[id] = RowError(
                message: String(localized: "Could not load saved host."),
                permissionDenied: false, expanded: false
            )
            if self.inFlightID == id { self.inFlightID = nil }
            return
        }
        let attempt = self.connectFactory()
        let outcome = await attempt.run(credential: entry.credential)
        if Task.isCancelled { return }

        switch outcome {
        case .session(let session):
            self.lastSession = session
            self.currentSessionID = id
        case .mismatch(let stored, let remote, let host, let port):
            self.pendingMismatch = PendingMismatch(
                stored: stored, remote: remote, host: host, port: port, sourceID: id
            )
        case .error(let msg, let permissionDenied):
            self.errorByID[id] = RowError(
                message: msg, permissionDenied: permissionDenied, expanded: false
            )
        }
        if self.inFlightID == id { self.inFlightID = nil }
    }

    /// Called after the user trusts a new host key on the mismatch screen.
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

    // MARK: - Errors

    public func toggleErrorExpansion(id: UUID) {
        guard var current = self.errorByID[id] else { return }
        current.expanded.toggle()
        self.errorByID[id] = current
    }

    public func collapseAllErrors() {
        for (id, var err) in self.errorByID where err.expanded {
            err.expanded = false
            self.errorByID[id] = err
        }
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
        self.errorByID[conf.targetID] = nil
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
