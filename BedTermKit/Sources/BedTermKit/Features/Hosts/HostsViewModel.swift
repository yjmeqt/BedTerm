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
    /// Installed by `HostsConnectController` against the persisted
    /// "show command blocks" setting (via `bt_ios_settings_*`). Nil keeps
    /// the channel pristine.
    public var bootstrapPayloadProvider: (@MainActor () -> String?)?

    private let store: HostsStore
    private let connectFactory: @MainActor () -> ConnectAttempt
    private var inFlightTask: Task<Void, Never>?

    public init(store: HostsStore = HostsStore()) {
        self.store = store
        self.connectFactory = {
            ConnectAttempt(clientFactory: {
                // UI-test override: when a launch-arg-driven stub is
                // registered, every production Connect tap routes
                // through the scripted `MockSSHClient` instead of
                // `CitadelSSHClient`. Production launches never set
                // this — see `SSHClientFactoryOverride`.
                if let factory = SSHClientFactoryOverride.current {
                    return factory()
                }
                return CitadelSSHClient()
            })
        }
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
        var loaded = self.store.list()
        // UI-test override: append injected stub hosts so the test can
        // tap a known row without driving the add-host form. Production
        // launches leave `HostsStoreInjection.current` empty.
        let injected = HostsStoreInjection.current.filter { entry in
            !loaded.contains { $0.id == entry.id }
        }
        loaded.append(contentsOf: injected)
        self.entries = loaded
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
        // UI-test override: injected stub hosts live only in memory,
        // not in the Keychain — short-circuit the store lookup so the
        // mock SSH client can answer the connect.
        if let injected = HostsStoreInjection.current.first(where: { $0.id == id }) {
            entry = injected
        } else {
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

    /// Disconnect the current live session (× tap on the terminal toolbar).
    /// Safe to call when there is no live session — falls through to
    /// `sessionEnded()`.
    public func endLiveSession() {
        guard let session = self.lastSession else {
            self.sessionEnded()
            return
        }
        session.disconnect()
        self.lastSession = nil
        self.currentSessionID = nil
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
