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
    private(set) var feed: AsyncStream<Data>
    private let feedContinuation: AsyncStream<Data>.Continuation
    private let client: any SSHClient
    private var pumpTask: Task<Void, Never>?

    init(client: any SSHClient) {
        self.client = client
        var continuation: AsyncStream<Data>.Continuation!
        self.feed = AsyncStream<Data> { continuation = $0 }
        self.feedContinuation = continuation
    }

    func connect(credential: HostCredential, initialPTY: PTYDimensions) async {
        state = .connecting
        do {
            try await client.connect(.init(credential: credential, initialPTY: initialPTY))
            state = .open
            pumpTask = Task { [feedContinuation, client] in
                for await chunk in client.output {
                    feedContinuation.yield(chunk)
                }
                await MainActor.run { [weak self] in
                    if case .open = self?.state {
                        self?.state = .closed(reason: "Disconnected")
                    }
                    self?.feedContinuation.finish()
                }
            }
        } catch let err as SSHError {
            state = .closed(reason: Self.describe(err))
        } catch {
            state = .closed(reason: String(describing: error))
        }
    }

    func send(_ data: Data) {
        guard case .open = state else { return }
        Task { try? await client.write(data) }
    }

    func resize(cols: Int, rows: Int) {
        guard case .open = state else { return }
        Task { try? await client.resize(.init(cols: cols, rows: rows)) }
    }

    func disconnect() {
        pumpTask?.cancel()
        Task { await client.disconnect() }
        feedContinuation.finish()
        state = .closed(reason: "Disconnected")
    }

    nonisolated static func describe(_ error: SSHError) -> String {
        switch error {
        case .dnsResolution: return "Cannot resolve host."
        case .tcpRefused: return "Connection refused — check host and port."
        case .timeout: return "Connection timed out."
        case .handshakeFailed(let reason): return "SSH handshake failed: \(reason)"
        case .authenticationFailed: return "Authentication failed."
        case .privateKeyParse: return "Cannot parse private key."
        case .privateKeyPassphraseRequired: return "Private key requires a passphrase."
        case .hostKeyMismatch(let stored, let remote):
            return "Host key changed.\nStored: \(stored)\nRemote: \(remote)"
        case .disconnected(let reason): return "Disconnected: \(reason)"
        case .shellExited(let code): return "Shell exited (\(code))."
        }
    }
}
