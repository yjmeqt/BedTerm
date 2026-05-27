//
// SSHClientBridge.swift — Swift→Rust bridge wrapper for `SSHClient`.
//
// Design: bedterm/docs/specs/swift-rust-ssh-bridge.md
//
// Wraps a Swift `SSHClient` instance behind a `BtSSHClientVTable` so the
// Rust-side `BtIosTerminalSession` can drive it without dragging Citadel
// into Rust. Each vtable entry hops onto the `@MainActor`, awaits the
// underlying `async throws` API, then fires the C-style completion back
// on the main queue.
//
// Credential routing: `BtSSHConnectRequest.credential_opaque` is a
// stable opaque key the Rust caller hands in. Swift maintains a process-
// wide side table (`credentialRegistry`) so a Swift caller (e.g. the
// SwiftUI host) can stash a `HostCredential` against a token *before*
// triggering `connect` on the Rust side. The Rust side never inspects
// the credential — it just relays the token.
//

import BedTermCoreC
import Foundation

/// Wraps an `SSHClient` for the C ABI bridge. Constructed by
/// `bt_swift_ssh_bridge_create(...)`, kept alive via an `Unmanaged`
/// retain that balances the Rust `(release)(ctx)` call inside
/// `SSHBridgeHandle::drop`.
@MainActor
final class SSHClientBridge {
    private let client: any SSHClient
    private var outputPump: Task<Void, Never>?
    private var outputSink: BtSSHOutputSink?
    private var outputSinkCtx: UnsafeMutableRawPointer?

    init(client: any SSHClient) {
        self.client = client
    }

    deinit {
        outputPump?.cancel()
    }

    // MARK: - Thunks

    internal func connect(
        request: BtSSHConnectRequest,
        completion: @escaping BtSSHCompletion,
        completionCtx: UnsafeMutableRawPointer?
    ) {
        guard let credential = credentialRegistry.lookup(request.credential_opaque) else {
            fireCompletion(
                completion, completionCtx,
                code: BtSSHResultOther,
                detail: String(localized: "Missing credential for connect.")
            )
            return
        }
        let cols = Int(request.cols)
        let rows = Int(request.rows)
        let bootstrap: String?
        if let cstr = request.bootstrap_payload {
            bootstrap = String(cString: cstr)
        } else {
            bootstrap = nil
        }
        let request = SSHConnectionRequest(
            credential: credential,
            initialPTY: PTYDimensions(cols: cols, rows: rows),
            bootstrapPayload: bootstrap
        )
        let client = self.client
        Task { @MainActor [weak self] in
            do {
                try await client.connect(request)
                // Kick the output pump now that the channel is open.
                self?.startOutputPumpIfNeeded()
                self?.fireCompletion(
                    completion, completionCtx, code: BtSSHResultOk, detail: nil)
            } catch let err as SSHError {
                let mapped = mapSSHError(err)
                self?.fireCompletion(
                    completion, completionCtx,
                    code: mapped.code, detail: mapped.detail, extra: mapped.extra)
            } catch {
                self?.fireCompletion(
                    completion, completionCtx,
                    code: BtSSHResultOther,
                    detail: String(describing: error))
            }
        }
    }

    internal func write(
        bytes: UnsafePointer<UInt8>?,
        length: Int,
        completion: @escaping BtSSHCompletion,
        completionCtx: UnsafeMutableRawPointer?
    ) {
        guard let bytes, length > 0 else {
            fireCompletion(completion, completionCtx, code: BtSSHResultOk, detail: nil)
            return
        }
        let data = Data(bytes: bytes, count: length)
        let client = self.client
        Task { @MainActor [weak self] in
            do {
                try await client.write(data)
                self?.fireCompletion(
                    completion, completionCtx, code: BtSSHResultOk, detail: nil)
            } catch let err as SSHError {
                let mapped = mapSSHError(err)
                self?.fireCompletion(
                    completion, completionCtx,
                    code: mapped.code, detail: mapped.detail, extra: mapped.extra)
            } catch {
                self?.fireCompletion(
                    completion, completionCtx,
                    code: BtSSHResultDisconnected,
                    detail: String(describing: error))
            }
        }
    }

    internal func resize(
        cols: Int32, rows: Int32,
        completion: @escaping BtSSHCompletion,
        completionCtx: UnsafeMutableRawPointer?
    ) {
        let dims = PTYDimensions(cols: Int(cols), rows: Int(rows))
        let client = self.client
        Task { @MainActor [weak self] in
            do {
                try await client.resize(dims)
                self?.fireCompletion(
                    completion, completionCtx, code: BtSSHResultOk, detail: nil)
            } catch let err as SSHError {
                let mapped = mapSSHError(err)
                self?.fireCompletion(
                    completion, completionCtx,
                    code: mapped.code, detail: mapped.detail, extra: mapped.extra)
            } catch {
                self?.fireCompletion(
                    completion, completionCtx,
                    code: BtSSHResultOther,
                    detail: String(describing: error))
            }
        }
    }

    internal func disconnect(
        completion: @escaping BtSSHCompletion,
        completionCtx: UnsafeMutableRawPointer?
    ) {
        let client = self.client
        Task { @MainActor [weak self] in
            await client.disconnect()
            self?.outputPump?.cancel()
            self?.outputPump = nil
            self?.fireCompletion(
                completion, completionCtx, code: BtSSHResultOk, detail: nil)
        }
    }

    internal func setOutputSink(
        _ sink: BtSSHOutputSink?, ctx: UnsafeMutableRawPointer?
    ) {
        self.outputSink = sink
        self.outputSinkCtx = ctx
        startOutputPumpIfNeeded()
    }

    // MARK: - Output pump

    private func startOutputPumpIfNeeded() {
        guard outputPump == nil, outputSink != nil else { return }
        let client = self.client
        outputPump = Task { @MainActor [weak self] in
            for await chunk in client.output {
                guard let self else { return }
                guard let sink = self.outputSink else { continue }
                let ctx = self.outputSinkCtx
                chunk.withUnsafeBytes { raw in
                    guard let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                    else { return }
                    sink(ctx, base, UInt(raw.count))
                }
            }
        }
    }

    // MARK: - Completion helpers

    private func fireCompletion(
        _ completion: @escaping BtSSHCompletion,
        _ ctx: UnsafeMutableRawPointer?,
        code: BtSSHResultCode,
        detail: String?,
        extra: Int32 = 0
    ) {
        let detailPtr: UnsafeRawPointer? = detail.map { msg in
            // Retain a NSString * for Rust to release via
            // `bt_ssh_release_message`.
            let ns = msg as NSString
            return UnsafeRawPointer(Unmanaged.passRetained(ns).toOpaque())
        }
        completion(ctx, code, detailPtr, extra)
    }
}

// MARK: - Credential side table

/// Process-wide side table mapping opaque credential tokens (raw pointers,
/// stable values produced by the Swift host) to `HostCredential`s. The
/// Rust side passes the token verbatim through `BtSSHConnectRequest`.
private final class CredentialRegistry: @unchecked Sendable {
    private var table: [UnsafeRawPointer: HostCredential] = [:]
    private let lock = NSLock()

    func register(_ credential: HostCredential) -> UnsafeRawPointer {
        // Use a freshly-allocated, single-byte sentinel as the token — its
        // address is unique for the registry's lifetime. We retain the box
        // by storing it in a separate keys-set; the credential is the value.
        let sentinel = UnsafeMutableRawPointer.allocate(byteCount: 1, alignment: 1)
        let opaque = UnsafeRawPointer(sentinel)
        lock.lock()
        table[opaque] = credential
        lock.unlock()
        return opaque
    }

    func lookup(_ token: UnsafeRawPointer?) -> HostCredential? {
        guard let token else { return nil }
        lock.lock()
        defer { lock.unlock() }
        return table[token]
    }

    func unregister(_ token: UnsafeRawPointer?) {
        guard let token else { return }
        lock.lock()
        _ = table.removeValue(forKey: token)
        lock.unlock()
        UnsafeMutableRawPointer(mutating: token).deallocate()
    }
}

private let credentialRegistry = CredentialRegistry()

/// Public Swift helper: register a `HostCredential` and return the opaque
/// token Rust will pass back through `BtSSHConnectRequest.credential_opaque`.
/// The token stays valid until `unregisterSSHCredential(_:)` is called.
public func registerSSHCredential(_ credential: HostCredential) -> UnsafeRawPointer {
    credentialRegistry.register(credential)
}

/// Drop a previously-registered credential token.
public func unregisterSSHCredential(_ token: UnsafeRawPointer?) {
    credentialRegistry.unregister(token)
}

// MARK: - Error mapping

private struct MappedSSHError {
    let code: BtSSHResultCode
    let detail: String?
    let extra: Int32
}

private func mapSSHError(_ err: SSHError) -> MappedSSHError {
    if let simple = mapSSHErrorSimple(err) {
        return simple
    }
    return mapSSHErrorWithDetails(err)
}

private func mapSSHErrorSimple(_ err: SSHError) -> MappedSSHError? {
    switch err {
    case .dnsResolution:
        return MappedSSHError(code: BtSSHResultDnsResolution, detail: nil, extra: 0)
    case .tcpRefused:
        return MappedSSHError(code: BtSSHResultTcpRefused, detail: nil, extra: 0)
    case .timeout:
        return MappedSSHError(code: BtSSHResultTimeout, detail: nil, extra: 0)
    case .authenticationFailed:
        return MappedSSHError(code: BtSSHResultAuthenticationFailed, detail: nil, extra: 0)
    case .privateKeyParse:
        return MappedSSHError(code: BtSSHResultPrivateKeyParse, detail: nil, extra: 0)
    case .privateKeyPassphraseRequired:
        return MappedSSHError(
            code: BtSSHResultPrivateKeyPassphraseRequired, detail: nil, extra: 0)
    case .peerReset:
        return MappedSSHError(code: BtSSHResultPeerReset, detail: nil, extra: 0)
    default:
        return nil
    }
}

private func mapSSHErrorWithDetails(_ err: SSHError) -> MappedSSHError {
    switch err {
    case .handshakeFailed(let msg):
        return MappedSSHError(code: BtSSHResultHandshakeFailed, detail: msg, extra: 0)
    case .hostKeyMismatch(let stored, let remote):
        return MappedSSHError(
            code: BtSSHResultHostKeyMismatch, detail: "\(stored)\n\(remote)", extra: 0)
    case .disconnected(let reason):
        return MappedSSHError(code: BtSSHResultDisconnected, detail: reason, extra: 0)
    case .shellExited(let code):
        return MappedSSHError(code: BtSSHResultShellExited, detail: nil, extra: Int32(code))
    default:
        return MappedSSHError(code: BtSSHResultOther, detail: nil, extra: 0)
    }
}

// C ABI thunks + `bt_swift_ssh_bridge_create` / `bt_ssh_release_message`
// live in `SSHClientBridge+Thunks.swift` to keep this file under
// SwiftLint's 400-line cap.
