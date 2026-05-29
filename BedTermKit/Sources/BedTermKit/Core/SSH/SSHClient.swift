import Foundation

public struct PTYDimensions: Equatable, Sendable {
    public var cols: Int
    public var rows: Int
    public init(cols: Int, rows: Int) {
        self.cols = cols
        self.rows = rows
    }
}

// MARK: - SSH error

public enum SSHError: Error, Equatable, Sendable {
    case dnsResolution
    case tcpRefused
    case timeout
    case handshakeFailed(String)
    case authenticationFailed
    case privateKeyParse
    case privateKeyPassphraseRequired
    case hostKeyMismatch(stored: String, remote: String)
    case disconnected(String)
    case peerReset
    case shellExited(Int)
}

extension SSHError {
    /// Returns a user-facing description string for this error via Rust FFI.
    public func describe() -> String {
        let (code, detail) = sshErrorCodeAndDetail(self)
        let buf: UnsafePointer<CChar>?
        if let detail {
            buf = detail.withCString { bt_ssh_error_describe(code, $0) }
        } else {
            buf = bt_ssh_error_describe(code, nil)
        }
        guard let buf else { return "SSH error (\(code))" }
        return String(cString: buf)
    }
}

private func sshErrorCodeAndDetail(_ error: SSHError) -> (Int32, String?) {
    switch error {
    case .dnsResolution: return (1, nil)
    case .tcpRefused: return (2, nil)
    case .timeout: return (3, nil)
    case .handshakeFailed(let reason): return (4, reason)
    case .authenticationFailed: return (5, nil)
    case .privateKeyParse: return (6, nil)
    case .privateKeyPassphraseRequired: return (7, nil)
    case .hostKeyMismatch(let stored, let remote): return (8, "stored: \(stored), remote: \(remote)")
    case .disconnected(let reason): return (9, reason)
    case .peerReset: return (10, nil)
    case .shellExited(let exitCode): return (11, "\(exitCode)")
    }
}

/// FFI declaration — maps a `BtSSHResultCode` and optional detail to a
/// static C string. The returned pointer is valid until the next call
/// from the same thread; the Swift `String(cString:)` initializer copies
/// the bytes immediately so this is safe.
@_silgen_name("bt_ssh_error_describe")
private func bt_ssh_error_describe(_ code: Int32, _ detail: UnsafePointer<CChar>?) -> UnsafePointer<CChar>?

public struct SSHConnectionRequest: Sendable {
    public let credential: HostCredential
    public let initialPTY: PTYDimensions
    /// Optional bootstrap payload — usually a heredoc-wrapped `eval` that
    /// sources the bundled OSC 133 shell-integration script. When non-nil
    /// and non-empty, the client writes these bytes into the channel as
    /// soon as the PTY is ready, before yielding any user input. The
    /// payload should end with a newline so the remote shell executes it
    /// immediately. `nil` keeps the channel pristine — the default.
    public let bootstrapPayload: String?

    public init(
        credential: HostCredential,
        initialPTY: PTYDimensions,
        bootstrapPayload: String? = nil
    ) {
        self.credential = credential
        self.initialPTY = initialPTY
        self.bootstrapPayload = bootstrapPayload
    }
}

public protocol SSHClient: AnyObject, Sendable {
    /// Stream of remote stdout/stderr bytes. The implementation should complete the stream when the session ends.
    var output: AsyncStream<Data> { get }

    func connect(_ request: SSHConnectionRequest) async throws
    func write(_ data: Data) async throws
    func resize(_ dims: PTYDimensions) async throws
    func disconnect() async
}
