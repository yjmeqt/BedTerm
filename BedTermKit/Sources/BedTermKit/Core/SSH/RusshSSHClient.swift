import BedTermIOS
import Foundation

private final class RusshCallbackBox: @unchecked Sendable {
    private let outputContinuation: AsyncStream<Data>.Continuation
    private let lock = NSLock()
    private var finished = false

    init(_ outputContinuation: AsyncStream<Data>.Continuation) {
        self.outputContinuation = outputContinuation
    }

    func yield(_ data: Data) {
        lock.lock()
        let shouldDrop = finished
        lock.unlock()
        if shouldDrop { return }
        outputContinuation.yield(data)
    }

    func finish() {
        lock.lock()
        let shouldFinish = !finished
        finished = true
        lock.unlock()
        if shouldFinish {
            outputContinuation.finish()
        }
    }
}

private let russhOutputSink:
    @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt) -> Void = { ctx, bytes, len in
        guard let ctx, let bytes, len > 0 else { return }
        let box = Unmanaged<RusshCallbackBox>.fromOpaque(ctx).takeUnretainedValue()
        box.yield(Data(bytes: bytes, count: Int(len)))
    }

private let russhCloseSink: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
    guard let ctx else { return }
    let box = Unmanaged<RusshCallbackBox>.fromOpaque(ctx).takeUnretainedValue()
    box.finish()
}

public final class RusshSSHClient: BedTermKit.SSHClient, @unchecked Sendable {
    public var output: AsyncStream<Data> { outputStream }

    private let outputStream: AsyncStream<Data>
    private let callbackBox: RusshCallbackBox
    private let callbackCtx: UnsafeMutableRawPointer
    private let client: OpaquePointer

    public init() {
        var continuation: AsyncStream<Data>.Continuation!
        outputStream = AsyncStream<Data> { continuation = $0 }
        callbackBox = RusshCallbackBox(continuation)
        callbackCtx = Unmanaged.passRetained(callbackBox).toOpaque()
        guard let client = bt_russh_client_create(
            russhOutputSink, russhCloseSink, callbackCtx)
        else {
            fatalError("failed to create russh client runtime")
        }
        self.client = client
    }

    deinit {
        bt_russh_client_release(client)
        Unmanaged<RusshCallbackBox>.fromOpaque(callbackCtx).release()
    }

    public func connect(_ request: SSHConnectionRequest) async throws {
        let rawClient = UInt(bitPattern: client)
        try await Task.detached(priority: .userInitiated) {
            try Self.throwIfNeeded(Self.connectBlocking(client: rawClient, request: request))
        }.value
    }

    public func write(_ data: Data) async throws {
        let rawClient = UInt(bitPattern: client)
        try await Task.detached(priority: .userInitiated) {
            try Self.throwIfNeeded(Self.writeBlocking(client: rawClient, data: data))
        }.value
    }

    public func resize(_ dims: PTYDimensions) async throws {
        let rawClient = UInt(bitPattern: client)
        let cols = UInt16(clamping: dims.cols)
        let rows = UInt16(clamping: dims.rows)
        try await Task.detached(priority: .userInitiated) {
            guard let client = OpaquePointer(bitPattern: rawClient) else {
                try Self.throwIfNeeded(Self.otherResult())
                return
            }
            try Self.throwIfNeeded(bt_russh_client_resize(client, cols, rows))
        }.value
    }

    public func disconnect() async {
        let rawClient = UInt(bitPattern: client)
        _ = await Task.detached(priority: .userInitiated) {
            guard let client = OpaquePointer(bitPattern: rawClient) else { return }
            _ = bt_russh_client_disconnect(client)
        }.value
        callbackBox.finish()
    }

    // swiftlint:disable:next function_body_length
    private static func connectBlocking(
        client rawClient: UInt,
        request: SSHConnectionRequest
    ) -> BtRusshResult {
        guard let client = OpaquePointer(bitPattern: rawClient) else {
            return otherResult()
        }
        let credential = request.credential
        return credential.host.withCString { hostPtr in
            credential.username.withCString { usernamePtr in
                withOptionalCString(request.bootstrapPayload) { bootstrapPtr in
                    switch credential.auth {
                    case .password(let password):
                        return password.withCString { passwordPtr in
                            var req = BtRusshConnectRequest(
                                host: hostPtr,
                                port: UInt16(clamping: credential.port),
                                username: usernamePtr,
                                auth_kind: BtRusshAuthPassword,
                                password: passwordPtr,
                                private_key: nil,
                                private_key_len: 0,
                                passphrase: nil,
                                cols: UInt16(clamping: request.initialPTY.cols),
                                rows: UInt16(clamping: request.initialPTY.rows),
                                bootstrap_payload: bootstrapPtr
                            )
                            return withUnsafePointer(to: &req) { reqPtr in
                                bt_russh_client_connect(client, reqPtr)
                            }
                        }
                    case .privateKey(let keyData, let passphrase):
                        return keyData.withUnsafeBytes { raw in
                            let keyPtr = raw.bindMemory(to: UInt8.self).baseAddress
                            return withOptionalCString(passphrase) { passphrasePtr in
                                var req = BtRusshConnectRequest(
                                    host: hostPtr,
                                    port: UInt16(clamping: credential.port),
                                    username: usernamePtr,
                                    auth_kind: BtRusshAuthPrivateKey,
                                    password: nil,
                                    private_key: keyPtr,
                                    private_key_len: UInt(raw.count),
                                    passphrase: passphrasePtr,
                                    cols: UInt16(clamping: request.initialPTY.cols),
                                    rows: UInt16(clamping: request.initialPTY.rows),
                                    bootstrap_payload: bootstrapPtr
                                )
                                return withUnsafePointer(to: &req) { reqPtr in
                                    bt_russh_client_connect(client, reqPtr)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    private static func writeBlocking(client rawClient: UInt, data: Data) -> BtRusshResult {
        guard let client = OpaquePointer(bitPattern: rawClient) else {
            return otherResult()
        }
        if data.isEmpty {
            return bt_russh_client_write(client, nil, 0)
        }
        return data.withUnsafeBytes { raw in
            let bytes = raw.bindMemory(to: UInt8.self).baseAddress
            return bt_russh_client_write(client, bytes, UInt(raw.count))
        }
    }

    private static func withOptionalCString<R>(
        _ string: String?,
        _ body: (UnsafePointer<CChar>?) -> R
    ) -> R {
        guard let string else { return body(nil) }
        return string.withCString(body)
    }

    // swiftlint:disable:next cyclomatic_complexity
    private static func throwIfNeeded(_ result: BtRusshResult) throws {
        let detail = result.message.map { String(cString: $0) }
        if let message = result.message {
            bt_russh_result_message_free(message)
        }
        if result.code == BtSSHResultOk { return }
        if result.code == BtSSHResultDnsResolution { throw SSHError.dnsResolution }
        if result.code == BtSSHResultTcpRefused { throw SSHError.tcpRefused }
        if result.code == BtSSHResultTimeout { throw SSHError.timeout }
        if result.code == BtSSHResultAuthenticationFailed {
            throw SSHError.authenticationFailed
        }
        if result.code == BtSSHResultPrivateKeyParse { throw SSHError.privateKeyParse }
        if result.code == BtSSHResultPrivateKeyPassphraseRequired {
            throw SSHError.privateKeyPassphraseRequired
        }
        if result.code == BtSSHResultPeerReset { throw SSHError.peerReset }
        if result.code == BtSSHResultHandshakeFailed {
            throw SSHError.handshakeFailed(detail ?? "")
        }
        if result.code == BtSSHResultHostKeyMismatch {
            let parts = (detail ?? "").split(separator: "\n", maxSplits: 1)
            let stored = parts.first.map(String.init) ?? ""
            let remote = parts.count > 1 ? String(parts[1]) : ""
            throw SSHError.hostKeyMismatch(stored: stored, remote: remote)
        }
        if result.code == BtSSHResultShellExited {
            throw SSHError.shellExited(Int(result.extra))
        }
        throw SSHError.disconnected(detail ?? "russh error")
    }

    private static func otherResult() -> BtRusshResult {
        BtRusshResult(code: BtSSHResultOther, message: nil, extra: 0)
    }
}
