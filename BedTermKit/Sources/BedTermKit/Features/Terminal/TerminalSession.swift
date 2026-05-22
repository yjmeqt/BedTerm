import Foundation
import Observation

@MainActor
@Observable
final class TerminalSession {
    enum State: Equatable {
        case idle
        case connecting
        case open
        case closed(reason: String)
    }

    private(set) var state: State = .idle
    private(set) var lastError: SSHError?
    /// Current terminal mode flags, mirrored from the Rust core after each
    /// feed by `TerminalMetalUIView`. Composer / overlays observe this to
    /// hide themselves when full-screen TUIs (vim, claude, htop) take over.
    private(set) var mode: BedTermMode = []
    private(set) var feed: AsyncStream<Data>
    private let feedContinuation: AsyncStream<Data>.Continuation
    /// Observable mirror of the Rust-owned block list. The session pump
    /// calls `blockStore.refresh(from: terminalCore)` after each
    /// `TerminalCore.feed(_:)`; canonical state (ids, boundaries, frozen
    /// snapshots) lives in Rust.
    public let blockStore = BlockStore()

    /// Authoritative terminal grid + scrollback. Owned by the session
    /// strongly so it outlives the Metal renderer view — re-entering a
    /// backgrounded session restores the previously drawn content
    /// (background-sessions P1: TerminalCore strong-ownership).
    /// The renderer view borrows this via the init and only displays it.
    @ObservationIgnored
    public let terminalCore: TerminalCore
    private let client: any SSHClient
    private var pumpTask: Task<Void, Never>?
    private var killRecorded = false

    /// SQLite snapshot row identifier for this session. Stable for the
    /// session lifetime; used to record blocks and kill metadata.
    let snapshotID: UUID
    /// The SavedHost UUID this session belongs to — written into the
    /// `snapshots` row at attach time.
    let hostID: UUID
    /// Weak reference to the process-wide persistence layer. Nil when
    /// running in debug/test contexts that opt out of persistence.
    @ObservationIgnored
    private weak var persistence: PersistenceHandle?

    /// Optional hook fired before each `send(_:)` writes to the PTY. Set by
    /// the terminal view to implement R5.scroll_snap_on_input (snap back to
    /// the live bottom whenever the user produces a byte while scrolled up).
    /// Excluded from `@Observable` tracking — it isn't a UI value.
    @ObservationIgnored
    var onBeforeSend: (() -> Void)?

    init(
        client: any SSHClient,
        hostID: UUID,
        persistence: PersistenceHandle?,
        snapshotID: UUID = UUID()
    ) {
        self.client = client
        self.hostID = hostID
        self.persistence = persistence
        self.snapshotID = snapshotID
        var feedCont: AsyncStream<Data>.Continuation!
        self.feed = AsyncStream<Data> { feedCont = $0 }
        self.feedContinuation = feedCont
        // Default geometry; first layout pass in TerminalMetalUIView
        // resizes to the actual viewport before any bytes arrive.
        self.terminalCore = TerminalCore(cols: 80, rows: 24)
        persistence?.attach(terminal: terminalCore, snapshotID: snapshotID, hostID: hostID)
    }

    func connect(
        credential: HostCredential,
        initialPTY: PTYDimensions,
        bootstrapPayload: String? = nil
    ) async {
        state = .connecting
        lastError = nil
        do {
            try await client.connect(
                .init(
                    credential: credential,
                    initialPTY: initialPTY,
                    bootstrapPayload: bootstrapPayload))
            state = .open
            pumpTask = Task { @MainActor [weak self] in
                guard let self else { return }
                for await chunk in self.client.output {
                    // Authoritative: feed the session-owned core so the
                    // grid + scrollback keep advancing even when the
                    // renderer view is unmounted (Back keeps session
                    // alive). The yield below is a redraw signal for
                    // the mounted view; bytes are not re-applied to
                    // core there.
                    self.terminalCore.feed(chunk)
                    self.updateMode(self.terminalCore.mode)
                    self.blockStore.refresh(from: self.terminalCore)
                    self.feedContinuation.yield(chunk)
                }
                if case .open = self.state {
                    self.recordKill(reason: .remoteLogout)
                    self.state = .closed(reason: String(localized: "Connection ended"))
                }
                self.feedContinuation.finish()
            }
        } catch let err as SSHError {
            lastError = err
            recordKill(reason: killReason(for: err))
            state = .closed(reason: Self.describe(err))
        } catch {
            recordKill(reason: .userKilled)
            state = .closed(reason: String(describing: error))
        }
    }

    func send(_ data: Data) {
        guard case .open = state else { return }
        onBeforeSend?()
        Task { try? await client.write(data) }
    }

    func resize(cols: Int, rows: Int) {
        guard case .open = state else { return }
        Task { try? await client.resize(.init(cols: cols, rows: rows)) }
    }

    /// Pushes a fresh mode snapshot from the renderer view. Only emits when
    /// the value actually changes so SwiftUI doesn't churn on identical reads.
    func updateMode(_ next: BedTermMode) {
        if mode != next { mode = next }
    }

    func disconnect() {
        recordKill(reason: .userKilled)
        pumpTask?.cancel()
        Task { await client.disconnect() }
        feedContinuation.finish()
        blockStore.reset()
        state = .closed(reason: String(localized: "Closed"))
    }

    // MARK: - Persistence helpers

    /// Write kill metadata to SQLite. Guards against double-writes via a
    /// one-shot boolean flag. This allows recordKill to be called from any
    /// state (.idle, .connecting, .open) and ensures it runs exactly once,
    /// blocking subsequent calls regardless of state.
    private func recordKill(reason: SessionSnapshot.KillReason) {
        guard !killRecorded else { return }
        killRecorded = true
        guard let persistence else { return }
        // Pull last-finalized block metadata for the snapshot row.
        let lastBlock = blockStore.blocks.last(where: { !$0.isRunning })
        persistence.recordKill(
            snapshotID: snapshotID,
            reason: reason,
            lastCwd: lastBlock?.workingDirectory,
            lastCommand: lastBlock?.command,
            lastExitCode: lastBlock?.exitCode
        )
    }

    private func killReason(for error: SSHError) -> SessionSnapshot.KillReason {
        switch error {
        case .peerReset, .dnsResolution, .tcpRefused, .timeout:
            return .networkDrop
        default:
            return .userKilled
        }
    }

    nonisolated static func describe(_ error: SSHError) -> String {
        if let simple = describeSimple(error) { return simple }
        return describeWithDetails(error)
    }

    private nonisolated static func describeSimple(_ error: SSHError) -> String? {
        switch error {
        case .dnsResolution:
            return String(localized: "Cannot resolve host.")
        case .tcpRefused:
            return String(localized: "Connection refused — check host and port.")
        case .timeout:
            return String(localized: "Connection timed out.")
        case .authenticationFailed:
            return String(localized: "Authentication failed.")
        case .privateKeyParse:
            return String(localized: "Cannot parse private key.")
        case .privateKeyPassphraseRequired:
            return String(localized: "Private key requires a passphrase.")
        case .peerReset:
            return String(
                localized: "Connection reset by the remote host (network change or idle timeout). Tap to reconnect.")
        default:
            return nil
        }
    }

    private nonisolated static func describeWithDetails(_ error: SSHError) -> String {
        switch error {
        case .handshakeFailed(let reason):
            return String(localized: "SSH handshake failed: \(reason)")
        case .hostKeyMismatch(let stored, let remote):
            return String(localized: "Host key changed.\nStored: \(stored)\nRemote: \(remote)")
        case .disconnected(let reason):
            return String(localized: "Disconnected: \(reason)")
        case .shellExited(let code):
            return String(localized: "Shell exited (\(code)).")
        default:
            return ""
        }
    }
}
