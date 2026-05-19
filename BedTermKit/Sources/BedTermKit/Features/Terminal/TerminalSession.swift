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
    /// Observable mirror of the Rust-owned block list. The renderer view
    /// calls `blockStore.refresh(from: terminalCore)` after each
    /// `TerminalCore.feed(_:)`; canonical state (ids, boundaries, frozen
    /// snapshots) lives in Rust.
    public let blockStore = BlockStore()

    /// The renderer view's `TerminalCore`, surfaced on the session so views
    /// outside the renderer hierarchy (Block list, future Block view) can
    /// read live grid state. Set by `TerminalMetalUIView` on init. Weak so
    /// we don't extend the renderer view's lifetime through this back-edge.
    @ObservationIgnored
    public weak var terminalCore: TerminalCore?
    private let client: any SSHClient
    private var pumpTask: Task<Void, Never>?

    /// Optional hook fired before each `send(_:)` writes to the PTY. Set by
    /// the terminal view to implement R5.scroll_snap_on_input (snap back to
    /// the live bottom whenever the user produces a byte while scrolled up).
    /// Excluded from `@Observable` tracking — it isn't a UI value.
    @ObservationIgnored
    var onBeforeSend: (() -> Void)?

    init(client: any SSHClient) {
        self.client = client
        var feedCont: AsyncStream<Data>.Continuation!
        self.feed = AsyncStream<Data> { feedCont = $0 }
        self.feedContinuation = feedCont
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
            pumpTask = Task { [feedContinuation, client] in
                for await chunk in client.output {
                    feedContinuation.yield(chunk)
                }
                await MainActor.run { [weak self] in
                    if case .open = self?.state {
                        self?.state = .closed(reason: String(localized: "Connection ended"))
                    }
                    self?.feedContinuation.finish()
                }
            }
        } catch let err as SSHError {
            lastError = err
            state = .closed(reason: Self.describe(err))
        } catch {
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
        pumpTask?.cancel()
        Task { await client.disconnect() }
        feedContinuation.finish()
        blockStore.reset()
        state = .closed(reason: String(localized: "Closed"))
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
