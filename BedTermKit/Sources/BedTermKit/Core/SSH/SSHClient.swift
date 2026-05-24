import Foundation

public struct PTYDimensions: Equatable, Sendable {
    public var cols: Int
    public var rows: Int
    public init(cols: Int, rows: Int) {
        self.cols = cols
        self.rows = rows
    }
}

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
