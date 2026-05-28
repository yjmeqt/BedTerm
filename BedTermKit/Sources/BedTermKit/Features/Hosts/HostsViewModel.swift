import BedTermIOS
import Foundation
import Observation
import UIKit

/// @Observable proxy over the Rust-owned hosts state machine
/// (`hosts_vm` / `bt_ios_hosts_vm_*`). Each mutator forwards to Rust,
/// mirrors the new state into the local @Observable properties so
/// `withObservationTracking` in `HostsConnectController` keeps firing
/// on the same key paths, and dispatches any `Action::Connect` / `Disconnect`
/// that came back.
///
/// Swift still owns the async SSH connect path (`ConnectAttempt`) and
/// the live `TerminalSession` reference — neither has a Rust analogue.
@MainActor
@Observable
public final class HostsViewModel {
    public private(set) var entries: [SavedHost] = []
    public private(set) var inFlightID: UUID?
    private(set) var lastSession: TerminalSession?
    public private(set) var currentSessionID: UUID?
    public var pendingMismatch: PendingMismatch?
    public var swapConfirmation: SwapConfirmation?
    public var deleteConfirmation: DeleteConfirmation?
    public private(set) var loadFailed: Bool = false

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

    private let connectFactory: @MainActor () -> ConnectAttempt
    private var inFlightTask: Task<Void, Never>?

    public init() {
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
        self.syncFromRust()
    }

    init(connectFactory: @MainActor @escaping () -> ConnectAttempt) {
        self.connectFactory = connectFactory
        self.syncFromRust()
    }

    // MARK: - Loading

    public func load() {
        if !UIApplication.shared.isProtectedDataAvailable {
            bt_ios_hosts_vm_set_load_failed(true)
            self.syncFromRust()
            return
        }
        bt_ios_hosts_vm_load_from_store()

        // UI-test override: inject stub hosts that don't live in the
        // Keychain. The merge has to happen Swift-side because Rust
        // doesn't see HostsStoreInjection's array.
        let injected = HostsStoreInjection.current
        if !injected.isEmpty {
            if let json = Self.encodeInjectedEntries(injected) {
                json.withCString { bt_ios_hosts_vm_merge_injected($0) }
            }
        }
        bt_ios_hosts_vm_set_load_failed(false)
        self.syncFromRust()
    }

    public func retryLoad() { self.load() }

    /// Entry point bound to the row's Connect button.
    public func requestConnect(id: UUID) {
        var out: UnsafeMutablePointer<CChar>?
        let kind = id.uuidString.withCString { idPtr in
            bt_ios_hosts_vm_request_connect(idPtr, &out)
        }
        self.syncFromRust()
        self.dispatch(actionKind: kind, payload: out)
    }

    public func confirmSwap() {
        var out: UnsafeMutablePointer<CChar>?
        bt_ios_hosts_vm_confirm_swap(&out)
        // Swift owns lastSession — disconnect *before* kicking off the
        // new attempt. The Rust VM has already cleared current_session_id.
        self.lastSession?.disconnect()
        self.lastSession = nil
        self.syncFromRust()
        if let out {
            let target = String(cString: out)
            bt_ios_hosts_free_string(out)
            if let uuid = UUID(uuidString: target) {
                self.startConnect(id: uuid)
            }
        }
    }

    public func cancelSwap() {
        bt_ios_hosts_vm_cancel_swap()
        self.syncFromRust()
    }

    // MARK: - Connect

    /// Async dispatch path called from the entry points above (and from
    /// `confirmSwap` / `retryAfterMismatch`). Rust already marked the
    /// in-flight id; Swift just runs the SSH attempt.
    private func startConnect(id: UUID) {
        self.inFlightTask?.cancel()
        let task = Task { @MainActor in
            await self.runConnect(id: id)
        }
        self.inFlightTask = task
    }

    /// Direct connect entry point (no swap dance). Used by tests that
    /// drive the connect path independently of `requestConnect`.
    public func connect(id: UUID) {
        var out: UnsafeMutablePointer<CChar>?
        let kind = id.uuidString.withCString { idPtr in
            bt_ios_hosts_vm_request_connect(idPtr, &out)
        }
        self.syncFromRust()
        // request_connect raises swap when a *different* session is
        // live; production callers want a direct dispatch. Force the
        // dispatch by bypassing the swap if it surfaced.
        if let out {
            bt_ios_hosts_free_string(out)
        }
        if kind == 0 && self.swapConfirmation == nil {
            // Same id as the live one (no-op) — nothing to do.
            return
        }
        if kind == 1 {
            // Direct dispatch — Rust already marked in_flight = id.
            self.startConnect(id: id)
        }
    }

    private func runConnect(id: UUID) async {
        let entry: SavedHost
        // UI-test override: injected stub hosts live only in memory.
        if let injected = HostsStoreInjection.current.first(where: { $0.id == id }) {
            entry = injected
        } else {
            guard
                let data = Self.loadHostJSON(id: id),
                let loaded = try? JSONDecoder().decode(SavedHost.self, from: data)
            else {
                id.uuidString.withCString { bt_ios_hosts_vm_connect_completed_error($0) }
                self.syncFromRust()
                self.onConnectError?(
                    id, String(localized: "Could not load saved host."), false)
                return
            }
            entry = loaded
        }
        let attempt = self.connectFactory()
        let outcome = await attempt.run(
            credential: entry.credential,
            bootstrapPayload: self.bootstrapPayloadProvider?())
        if Task.isCancelled { return }

        switch outcome {
        case .session(let session):
            self.lastSession = session
            id.uuidString.withCString { bt_ios_hosts_vm_connect_completed_session($0) }
        case .mismatch(let stored, let remote, let host, let port):
            id.uuidString.withCString { idPtr in
                stored.withCString { storedPtr in
                    remote.withCString { remotePtr in
                        host.withCString { hostPtr in
                            bt_ios_hosts_vm_connect_completed_mismatch(
                                idPtr, storedPtr, remotePtr, hostPtr, UInt16(port))
                        }
                    }
                }
            }
        case .error(let message, let permissionDenied):
            id.uuidString.withCString { bt_ios_hosts_vm_connect_completed_error($0) }
            self.onConnectError?(id, message, permissionDenied)
        }
        self.syncFromRust()
    }

    /// Called after the user trusts a new host key on the mismatch sheet.
    public func retryAfterMismatch() {
        var out: UnsafeMutablePointer<CChar>?
        let kind = bt_ios_hosts_vm_retry_after_mismatch(&out)
        self.syncFromRust()
        self.dispatch(actionKind: kind, payload: out)
    }

    public func clearMismatch() {
        bt_ios_hosts_vm_clear_mismatch()
        self.syncFromRust()
    }

    /// Called when the terminal screen tears down so the next row tap starts fresh.
    public func sessionEnded() {
        self.lastSession = nil
        bt_ios_hosts_vm_session_ended()
        self.syncFromRust()
    }

    /// Disconnect the current live session.
    public func endLiveSession() {
        let kind = bt_ios_hosts_vm_end_live_session()
        if kind == 2 {
            self.lastSession?.disconnect()
        }
        self.lastSession = nil
        self.syncFromRust()
    }

    // MARK: - Delete

    public func requestDelete(id: UUID) {
        id.uuidString.withCString { bt_ios_hosts_vm_request_delete($0) }
        self.syncFromRust()
    }

    public func confirmDelete() {
        guard let targetPtr = bt_ios_hosts_vm_confirm_delete() else {
            self.syncFromRust()
            return
        }
        let target = String(cString: targetPtr)
        bt_ios_hosts_free_string(targetPtr)
        guard let uuid = UUID(uuidString: target) else {
            self.syncFromRust()
            return
        }
        // Rust already cleared in-flight / live mirrors when they
        // matched. Mirror that on the Swift side too.
        if self.inFlightID == uuid {
            self.inFlightTask?.cancel()
        }
        if self.currentSessionID == uuid {
            self.lastSession?.disconnect()
            self.lastSession = nil
        }
        uuid.uuidString.withCString { bt_ios_hosts_delete($0) }
        self.syncFromRust()
    }

    public func cancelDelete() {
        bt_ios_hosts_vm_cancel_delete()
        self.syncFromRust()
    }

    // MARK: - Display helpers

    public func displayName(for id: UUID) -> String {
        guard let ptr = id.uuidString.withCString({ bt_ios_hosts_vm_display_name_for($0) }) else {
            return ""
        }
        defer { bt_ios_hosts_free_string(ptr) }
        return String(cString: ptr)
    }

    /// Reload after a form save so a freshly-added entry appears.
    public func refresh() {
        bt_ios_hosts_vm_load_from_store()
        self.syncFromRust()
    }

    // MARK: - Internal

    /// Mirror Rust VM state into @Observable properties so
    /// `withObservationTracking` fires on each change.
    private func syncFromRust() {
        let entriesJSONPtr = bt_ios_hosts_vm_entries_json()
        defer { bt_ios_hosts_free_string(entriesJSONPtr) }
        let entriesJSON = entriesJSONPtr.map { String(cString: $0) } ?? "[]"
        let mirror = Self.decodeEntryMirror(entriesJSON)

        var resolved: [SavedHost] = []
        resolved.reserveCapacity(mirror.count)
        for ref in mirror {
            if let injected = HostsStoreInjection.current.first(where: { $0.id == ref.id }) {
                resolved.append(injected)
                continue
            }
            if let entry = Self.loadHostJSON(id: ref.id).flatMap({
                try? JSONDecoder().decode(SavedHost.self, from: $0)
            }) {
                resolved.append(entry)
            }
        }
        self.entries = resolved

        self.inFlightID = Self.readOptionalUUID(bt_ios_hosts_vm_in_flight_id())
        self.currentSessionID = Self.readOptionalUUID(bt_ios_hosts_vm_current_session_id())
        self.pendingMismatch = Self.readOptionalJSON(bt_ios_hosts_vm_pending_mismatch_json())
        self.swapConfirmation = Self.readOptionalJSON(bt_ios_hosts_vm_swap_confirmation_json())
        self.deleteConfirmation = Self.readOptionalJSON(bt_ios_hosts_vm_delete_confirmation_json())
        self.loadFailed = bt_ios_hosts_vm_load_failed()
    }

    private func dispatch(actionKind: Int32, payload: UnsafeMutablePointer<CChar>?) {
        defer {
            if let payload { bt_ios_hosts_free_string(payload) }
        }
        switch actionKind {
        case 1:
            guard let payload else { return }
            let idString = String(cString: payload)
            guard let uuid = UUID(uuidString: idString) else { return }
            self.startConnect(id: uuid)
        case 2:
            self.lastSession?.disconnect()
            self.lastSession = nil
        default:
            break
        }
    }

    // MARK: - JSON helpers

    /// Load the full JSON blob for a single host entry from Rust's Keychain
    /// store. Returns nil when the entry doesn't exist or isn't valid UTF-8.
    private static func loadHostJSON(id: UUID) -> Data? {
        guard let ptr = id.uuidString.withCString({ bt_ios_hosts_load_json($0) }) else {
            return nil
        }
        defer { bt_ios_hosts_free_string(ptr) }
        return String(cString: ptr).data(using: .utf8)
    }

    private static func decodeEntryMirror(_ json: String) -> [HostsEntryRef] {
        guard let data = json.data(using: .utf8) else { return [] }
        return (try? JSONDecoder().decode([HostsEntryRef].self, from: data)) ?? []
    }

    private static func readOptionalUUID(_ ptr: UnsafeMutablePointer<CChar>?) -> UUID? {
        guard let ptr else { return nil }
        defer { bt_ios_hosts_free_string(ptr) }
        return UUID(uuidString: String(cString: ptr))
    }

    private static func readOptionalJSON<T: Decodable>(
        _ ptr: UnsafeMutablePointer<CChar>?
    ) -> T? {
        guard let ptr else { return nil }
        defer { bt_ios_hosts_free_string(ptr) }
        let json = String(cString: ptr)
        guard let data = json.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }

    /// Build a JSON array matching the display snapshot shape for the
    /// UI-test stub merge. Each entry maps to `{id, label, host, port,
    /// username, authIsKey}` — same fields Rust's `EntryMeta` parses.
    private static func encodeInjectedEntries(_ entries: [SavedHost]) -> String? {
        var items: [[String: Any]] = []
        items.reserveCapacity(entries.count)
        for entry in entries {
            let authIsKey: Bool
            switch entry.credential.auth {
            case .privateKey: authIsKey = true
            case .password: authIsKey = false
            }
            items.append([
                "id": entry.id.uuidString,
                "label": entry.label,
                "host": entry.credential.host,
                "port": entry.credential.port,
                "username": entry.credential.username,
                "authIsKey": authIsKey
            ])
        }
        guard
            let data = try? JSONSerialization.data(withJSONObject: items, options: []),
            let json = String(data: data, encoding: .utf8)
        else { return nil }
        return json
    }
}
