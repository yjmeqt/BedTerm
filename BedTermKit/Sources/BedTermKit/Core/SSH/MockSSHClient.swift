import Foundation

public final class MockSSHClient: SSHClient, @unchecked Sendable {
    public private(set) var written: [Data] = []
    public private(set) var lastResize: PTYDimensions?
    public private(set) var connectCalls = 0
    public private(set) var disconnectCalls = 0
    /// The most recent connect request, captured so tests can assert on
    /// fields the real client uses internally (bootstrap payload, initial
    /// PTY dimensions, credential type).
    public private(set) var lastConnectRequest: SSHConnectionRequest?

    private var scriptedOutput: [Data] = []
    private var scriptedConnectError: SSHError?
    private var scriptedConnectHang = false
    /// When true, every `write(_:)` call is echoed back through the
    /// `output` stream wrapped in a bright-magenta SGR run. Used by the
    /// end-to-end UI tests to confirm that keybar / composer taps reach
    /// the production `SSHClient` — pixel diff before/after the tap.
    private var echoesWrites = false
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
    /// Make `connect()` suspend until the surrounding task is cancelled. Lets
    /// tests exercise the connect-timeout race without hitting the network.
    public func scriptConnectHang() { self.scriptedConnectHang = true }
    /// Enable echo-on-write mode. Subsequent `write(_:)` calls yield
    /// a bright-magenta SGR-wrapped echo on `output`, so a UI-test
    /// screenshot can detect that the byte made it through the
    /// production SSH path.
    public func scriptEchoOnWrite() { self.echoesWrites = true }

    public func connect(_ request: SSHConnectionRequest) async throws {
        self.connectCalls += 1
        self.lastConnectRequest = request
        if let err = scriptedConnectError { throw err }
        if self.scriptedConnectHang {
            try await Task.sleep(nanoseconds: 60 * 1_000_000_000)
        }
        for chunk in self.scriptedOutput {
            self.continuation?.yield(chunk)
        }
    }

    public func write(_ data: Data) async throws {
        self.written.append(data)
        if self.echoesWrites, let continuation = self.continuation {
            // Emit one visible printable marker per inbound byte
            // wrapped in bright-magenta SGR, so even non-printing
            // control bytes (tab / esc / ctrl-letters) cause a
            // measurable pixel change on the metal view. A trailing
            // SGR reset keeps the subsequent terminal output clean.
            var framed = Data()
            framed.append(contentsOf: [0x1B, 0x5B, 0x39, 0x35, 0x6D])  // ESC[95m
            for byte in data {
                // Printable ASCII (space..tilde) renders as-is; every
                // other byte (control / non-ASCII) becomes a `#` so
                // the pixel buffer always sees a glyph for each
                // delivered byte.
                if byte >= 0x20 && byte <= 0x7E {
                    framed.append(byte)
                } else {
                    framed.append(0x23)  // '#'
                }
            }
            framed.append(contentsOf: [0x1B, 0x5B, 0x30, 0x6D])  // ESC[0m
            continuation.yield(framed)
        }
    }
    public func resize(_ dims: PTYDimensions) async throws { self.lastResize = dims }

    public func disconnect() async {
        self.disconnectCalls += 1
        self.continuation?.finish()
    }
}
