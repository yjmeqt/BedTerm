//
// CitadelSSHClient.swift — Production SSH client on Citadel 0.12.1 + NIOSSH.
//
// API notes verified against Citadel 0.12.1 / NIOSSH (2026-05-16, sources
// under DerivedData/.../SourcePackages/checkouts/{Citadel,swift-nio-ssh}):
//
//   - Citadel's own `SSHClient` final class is aliased to `CitadelClient`
//     to disambiguate from `BedTerm.SSHClient` protocol.
//   - Connect entry point used here:
//         Citadel.SSHClient.connect(
//             host:port:authenticationMethod:hostKeyValidator:reconnect:
//             algorithms:protocolOptions:group:channelHandlers:connectTimeout:
//         ) async throws -> SSHClient
//   - Auth factory:
//         SSHAuthenticationMethod.passwordBased(username:password:)
//         SSHAuthenticationMethod.ed25519(username:privateKey:)
//         SSHAuthenticationMethod.rsa(username:privateKey:)
//     Private keys are parsed from OpenSSH PEM strings via:
//         Curve25519.Signing.PrivateKey(sshEd25519: Data, decryptionKey: Data?)
//         Insecure.RSA.PrivateKey(sshRsa: Data, decryptionKey: Data?)
//     `decryptionKey` is the bcrypt passphrase bytes (NOT a derived key).
//   - Host-key validation: SSHHostKeyValidator wraps a
//     NIOSSHClientServerAuthenticationDelegate via `.custom(_:)`. The delegate's
//     `validateHostKey(hostKey:validationCompletePromise:)` runs on the channel
//     event loop, so we keep TOFU work cheap and non-blocking.
//   - PTY: we use Citadel's `withPTY(_:perform:)` (iOS gets it unconditionally;
//     it is `@available(macOS 15.0, *)` only). The closure receives a
//     `TTYOutput` (stdout/stderr AsyncSequence) and a `TTYStdinWriter`
//     (`.write(ByteBuffer)`, `.changeSize(cols:rows:pixelWidth:pixelHeight:)`).
//     The closure stays alive for the lifetime of the channel; we resume the
//     caller of `connect` once we've captured the writer, and we hold the
//     closure suspended on a `disconnectGate` continuation until `disconnect()`
//     is called.
//   - Disconnect: closing the PTY channel is handled by letting `withPTY`'s
//     closure return; we also call `Citadel.SSHClient.close()` afterwards.
//
// Limitations / caveats:
//   - Citadel 0.12.1 only ships ed25519 + RSA OpenSSH parsers; P256/P384/
//     P521 map to `.privateKeyParse`. Password + the two key types cover MVP.
//   - Citadel's `InvalidOpenSSHKey` does not distinguish "wrong passphrase"
//     from "garbage input". We surface `.privateKeyPassphraseRequired` only
//     when the caller supplied no passphrase and the key parser fails; if the
//     caller supplied a passphrase and parsing still fails we surface
//     `.privateKeyParse` (could be wrong passphrase or malformed key — Citadel
//     does not tell us which).
//   - Host-key TOFU is synchronous inside the validation callback (cheap
//     Keychain calls, matches `SSHHostKeyValidator.trustedKeys`).

@preconcurrency import Citadel
import Crypto
import Foundation
@preconcurrency import NIOCore
@preconcurrency import NIOSSH

// Disambiguate Citadel's `SSHClient` class from our `SSHClient` protocol.
private typealias CitadelClient = Citadel.SSHClient

public final class CitadelSSHClient: BedTermKit.SSHClient, @unchecked Sendable {
    public var output: AsyncStream<Data> { self.outputStream }

    private let outputStream: AsyncStream<Data>
    private let outputContinuation: AsyncStream<Data>.Continuation

    private let hostKeyStore: HostKeyStore

    private var client: CitadelClient?
    private var writer: TTYStdinWriter?
    private var sessionTask: Task<Void, Never>?
    private var disconnectGate: CheckedContinuation<Void, Never>?
    private var isDisconnected = false

    public init(hostKeyStore: HostKeyStore = HostKeyStore()) {
        self.hostKeyStore = hostKeyStore
        var continuation: AsyncStream<Data>.Continuation!
        self.outputStream = AsyncStream<Data> { continuation = $0 }
        self.outputContinuation = continuation
    }

    // MARK: - Connect

    // swiftlint:disable:next cyclomatic_complexity function_body_length
    public func connect(_ request: SSHConnectionRequest) async throws {
        let credential = request.credential

        let authMethod: SSHAuthenticationMethod
        do {
            authMethod = try Self.makeAuthMethod(for: credential)
        } catch let sshError as SSHError {
            throw sshError
        } catch {
            throw SSHError.privateKeyParse
        }

        let validator = TOFUHostKeyDelegate(
            store: self.hostKeyStore,
            host: credential.host,
            port: credential.port
        )
        let hostKeyValidator = SSHHostKeyValidator.custom(validator)

        let citadel: CitadelClient
        do {
            citadel = try await CitadelClient.connect(
                host: credential.host,
                port: credential.port,
                authenticationMethod: authMethod,
                hostKeyValidator: hostKeyValidator,
                reconnect: .never,
                channelHandlers: [TCPKeepaliveHandler()]
            )
        } catch let mismatch as TOFUHostKeyDelegate.Mismatch {
            throw SSHError.hostKeyMismatch(stored: mismatch.stored, remote: mismatch.remote)
        } catch let sshError as SSHError {
            throw sshError
        } catch {
            throw Self.classifyConnectError(error)
        }

        self.client = citadel

        let dims = request.initialPTY
        let ptyRequest = SSHChannelRequestEvent.PseudoTerminalRequest(
            wantReply: true,
            term: "xterm-256color",
            terminalCharacterWidth: dims.cols,
            terminalRowHeight: dims.rows,
            terminalPixelWidth: 0,
            terminalPixelHeight: 0,
            terminalModes: SSHTerminalModes([:])
        )

        // Bridge the closure-based `withPTY` into an open-ended channel:
        // suspend the caller of connect() until the PTY is ready (we have a
        // `TTYStdinWriter`), then keep the closure alive on `disconnectGate`.
        try await withCheckedThrowingContinuation { (ready: CheckedContinuation<Void, Error>) in
            self.sessionTask = Task { [weak self] in
                guard let self else {
                    ready.resume(throwing: SSHError.disconnected("client deallocated"))
                    return
                }
                do {
                    try await citadel.withPTY(ptyRequest) { inbound, outbound in
                        self.writer = outbound
                        self.maybeResume(ready, with: .success(()))

                        // Pump inbound stdout/stderr into our AsyncStream
                        // AND defer the bootstrap push until the remote
                        // shell produces its first byte. Writing the
                        // bootstrap into `outbound` BEFORE the inbound
                        // pump is active is observed to silently drop
                        // the bytes on some Citadel + sshd combinations
                        // (probe sentinel never returns). Waiting for
                        // first-output guarantees the shell has finished
                        // PTY setup, loaded `.zshrc`, and is reading from
                        // stdin in ZLE mode — the bootstrap heredoc then
                        // arrives as ordinary user input and gets eval'd.
                        let bootstrapPayload = request.bootstrapPayload
                        let pump = Task { [weak self] in
                            guard let self else { return }
                            var bootstrapPushed = false
                            do {
                                for try await chunk in inbound {
                                    switch chunk {
                                    case let .stdout(buffer), let .stderr(buffer):
                                        let data = Data(buffer.readableBytesView)
                                        logChunk("IN ", data)
                                        self.outputContinuation.yield(data)
                                    }
                                    if !bootstrapPushed {
                                        bootstrapPushed = true
                                        Task { [citadel] in
                                            await Self.pushBootstrap(
                                                payload: bootstrapPayload,
                                                writer: outbound,
                                                ssh: citadel)
                                        }
                                    }
                                }
                            } catch {
                                // Stream ended (EOF or remote exit) — fall
                                // through; the gate below will be released by
                                // disconnect() or by us once pump finishes.
                            }
                        }

                        // Wait until disconnect() is invoked OR the inbound
                        // pump finishes (remote-initiated EOF). We race the
                        // two via a child task that releases the gate.
                        await withCheckedContinuation { (gate: CheckedContinuation<Void, Never>) in
                            self.disconnectGate = gate
                            Task {
                                _ = await pump.value
                                self.releaseDisconnectGate()
                            }
                        }
                        pump.cancel()
                    }
                } catch {
                    // If we have not resumed `ready` yet, surface the error
                    // through it; otherwise the session simply ends.
                    self.maybeResume(ready, with: .failure(Self.classifyConnectError(error)))
                }
                self.outputContinuation.finish()
            }
        }
    }

    // `ready` may already have been resumed by the time withPTY throws. Guard
    // against double-resume by tracking it via a one-shot lock — both the
    // success path (PTY ready) and the failure path (withPTY threw before/
    // after ready) funnel through here so the continuation is only resumed once.
    private let readyLock = NSLock()
    private var readyResumed = false
    private func maybeResume(
        _ cont: CheckedContinuation<Void, Error>,
        with result: Result<Void, Error>
    ) {
        self.readyLock.lock()
        let already = self.readyResumed
        self.readyResumed = true
        self.readyLock.unlock()
        if already { return }
        cont.resume(with: result)
    }

    private func releaseDisconnectGate() {
        if let gate = self.disconnectGate {
            self.disconnectGate = nil
            gate.resume()
        }
    }

    // MARK: - Write / Resize / Disconnect

    public func write(_ data: Data) async throws {
        guard let writer = self.writer else {
            throw SSHError.disconnected("not connected")
        }
        logChunk("OUT", data)
        var buffer = ByteBufferAllocator().buffer(capacity: data.count)
        buffer.writeBytes(data)
        do {
            try await writer.write(buffer)
        } catch {
            throw SSHErrorMapping.map(error)
        }
    }

    public func resize(_ dims: PTYDimensions) async throws {
        guard let writer = self.writer else {
            throw SSHError.disconnected("not connected")
        }
        do {
            try await writer.changeSize(
                cols: dims.cols,
                rows: dims.rows,
                pixelWidth: 0,
                pixelHeight: 0
            )
        } catch {
            throw SSHErrorMapping.map(error)
        }
    }

    public func disconnect() async {
        if self.isDisconnected { return }
        self.isDisconnected = true

        // Let withPTY's closure return so Citadel closes the channel.
        self.releaseDisconnectGate()

        // Then close the underlying SSH session.
        if let client = self.client {
            try? await client.close()
        }
        self.client = nil
        self.writer = nil
    }

    // MARK: - Helpers

    private static func makeAuthMethod(for credential: HostCredential) throws -> SSHAuthenticationMethod {
        switch credential.auth {
        case let .password(password):
            return SSHAuthenticationMethod.passwordBased(
                username: credential.username,
                password: password
            )

        case let .privateKey(keyData, passphrase):
            let passphraseData = passphrase.flatMap { Data($0.utf8) }

            // Try ed25519 first, then RSA. Citadel 0.12.1 ships only these two
            // OpenSSH private-key parsers; other curves throw `.privateKeyParse`.
            if let ed = try? Curve25519.Signing.PrivateKey(
                sshEd25519: keyData,
                decryptionKey: passphraseData
            ) {
                return SSHAuthenticationMethod.ed25519(
                    username: credential.username,
                    privateKey: ed
                )
            }

            do {
                let rsa = try Insecure.RSA.PrivateKey(
                    sshRsa: keyData,
                    decryptionKey: passphraseData
                )
                return SSHAuthenticationMethod.rsa(
                    username: credential.username,
                    privateKey: rsa
                )
            } catch {
                // Distinguish "needs passphrase" from "garbage / wrong passphrase":
                // if caller supplied no passphrase but the failure mentions the
                // KDF/cipher path, ask for one. Otherwise report a parse error.
                if passphraseData == nil, Self.errorMentionsEncryption(error) {
                    throw SSHError.privateKeyPassphraseRequired
                }
                throw SSHError.privateKeyParse
            }
        }
    }

    private static func errorMentionsEncryption(_ error: Error) -> Bool {
        let text = String(describing: error).lowercased()
        return text.contains("bcrypt") || text.contains("cipher") || text.contains("decrypt") || text.contains("kdf")
            || text.contains("missingdecryptionkey")
    }

    private static func classifyConnectError(_ error: Error) -> SSHError {
        if let ssh = error as? SSHError { return ssh }
        if error is NIOSSH.NIOSSHError {
            let text = String(describing: error).lowercased()
            if text.contains("auth") { return .authenticationFailed }
            return .handshakeFailed(String(describing: error))
        }
        if error is InvalidHostKey {
            // Should be caught earlier via TOFUHostKeyDelegate.Mismatch, but be safe.
            return .hostKeyMismatch(stored: "", remote: "")
        }
        return SSHErrorMapping.map(error)
    }

}

// MARK: - TOFU host-key delegate

/// Trust-on-first-use host-key validator backed by `HostKeyStore`.
///
/// - On `.match`: accept.
/// - On `.unknown`: store and accept (the "trust on first use" leg).
/// - On `.mismatch`: fail the validation promise with a `Mismatch` carrying the
///   stored/remote fingerprints so `connect(_:)` can surface
///   `SSHError.hostKeyMismatch`.
private final class TOFUHostKeyDelegate: NIOSSHClientServerAuthenticationDelegate, @unchecked Sendable {
    struct Mismatch: Error {
        let stored: String
        let remote: String
    }

    private let store: HostKeyStore
    private let host: String
    private let port: Int

    init(store: HostKeyStore, host: String, port: Int) {
        self.store = store
        self.host = host
        self.port = port
    }

    func validateHostKey(
        hostKey: NIOSSHPublicKey,
        validationCompletePromise: EventLoopPromise<Void>
    ) {
        let fingerprint = Self.sha256Fingerprint(of: hostKey)
        do {
            switch try self.store.verify(remote: fingerprint, host: self.host, port: self.port) {
            case .match:
                validationCompletePromise.succeed(())
            case .unknown:
                try self.store.store(fingerprint: fingerprint, host: self.host, port: self.port)
                validationCompletePromise.succeed(())
            case let .mismatch(stored, remote):
                validationCompletePromise.fail(Mismatch(stored: stored, remote: remote))
            }
        } catch {
            validationCompletePromise.fail(error)
        }
    }

    /// SHA256:<base64-no-padding> over the raw SSH host-key blob, matching
    /// the format printed by `ssh-keygen -lf` / OpenSSH's
    /// `VisualHostKeyDisplay`.
    static func sha256Fingerprint(of key: NIOSSHPublicKey) -> String {
        // `String(openSSHPublicKey:)` returns "ssh-ed25519 BASE64". Pull the
        // base64 component and decode to get the raw key blob.
        let openSSH = String(openSSHPublicKey: key)
        let parts = openSSH.split(separator: " ", maxSplits: 1)
        let blob: Data =
            if parts.count == 2, let decoded = Data(base64Encoded: String(parts[1])) {
                decoded
            } else {
                Data(openSSH.utf8)
            }
        let digest = SHA256.hash(data: blob)
        let base64 = Data(digest).base64EncodedString()
            .trimmingCharacters(in: CharacterSet(charactersIn: "="))
        return "SHA256:\(base64)"
    }
}
