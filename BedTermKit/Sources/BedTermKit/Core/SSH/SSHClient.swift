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
    public init(credential: HostCredential, initialPTY: PTYDimensions) {
        self.credential = credential
        self.initialPTY = initialPTY
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
