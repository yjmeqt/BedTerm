import Foundation

public final class MockSSHClient: SSHClient {
    public private(set) var written: [Data] = []
    public private(set) var lastResize: PTYDimensions?
    public private(set) var connectCalls = 0
    public private(set) var disconnectCalls = 0

    private var scriptedOutput: [Data] = []
    private var scriptedConnectError: SSHError?
    private var continuation: AsyncStream<Data>.Continuation?
    private let outputStream: AsyncStream<Data>

    public var output: AsyncStream<Data> { self.outputStream }

    public init() {
        var continuation: AsyncStream<Data>.Continuation!
        self.outputStream = AsyncStream<Data> { continuation = $0 }
        self.continuation = continuation
    }

    public func script(output chunks: [Data]) { self.scriptedOutput = chunks }
    public func scriptConnectError(_ error: SSHError) { self.scriptedConnectError = error }

    public func connect(_ request: SSHConnectionRequest) async throws {
        self.connectCalls += 1
        if let err = scriptedConnectError { throw err }
        for chunk in self.scriptedOutput {
            self.continuation?.yield(chunk)
        }
    }

    public func write(_ data: Data) async throws { self.written.append(data) }
    public func resize(_ dims: PTYDimensions) async throws { self.lastResize = dims }

    public func disconnect() async {
        self.disconnectCalls += 1
        self.continuation?.finish()
    }
}
