import BedTermIOS
import Foundation
import Observation

/// SSH terminal session backed directly by the Rust `BtTerminalSessionHandle`
/// FFI. Replaces `TerminalSession` for the production connect path.
///
/// Architecture
/// ------------
/// 1. `init()` creates a Rust handle, installs a state callback and a data sink.
/// 2. `connect(...)` dispatches the blocking `bt_terminal_session_connect` to
///    `ffiQueue`, bridges the result back via a `CheckedThrowingContinuation`,
///    then sets `state = .open` synchronously on the MainActor.
/// 3. A `pumpTask` running on the MainActor feeds `internalStream` chunks into
///    `terminalCore`, updates `mode`, and forwards bytes to `feedContinuation`.
/// 4. Subsequent state changes (read-loop EOF / disconnect) are delivered by
///    the Rust state callback → `StateCallbackBox.onState` → MainActor task →
///    `onStateChange(...)`.
///
/// Thread safety
/// -------------
/// `@MainActor` covers all public surface. The Rust callbacks fire on arbitrary
/// threads; they only write into boxes that the MainActor reads later — they
/// never touch `RustTerminalSession` properties directly.
@MainActor
@Observable
public final class RustTerminalSession: @unchecked Sendable {
    // MARK: - State

    public enum State: Equatable {
        case idle
        case connecting
        case open
        case closed(reason: String)
    }

    public private(set) var state: State = .idle
    public private(set) var lastError: SSHError?

    /// Current terminal mode flags — updated after each feed chunk.
    public private(set) var mode: BedTermMode = []

    /// Authoritative terminal grid + scrollback owned by this session.
    @ObservationIgnored
    public let terminalCore: TerminalCore

    /// Byte stream consumed by `IosTerminalHost.feedTask`. Each element
    /// is a raw PTY chunk already applied to `terminalCore`.
    public var feed: AsyncStream<Data> { feedStream }

    /// Hook fired before each `send(_:)` call (R5 scroll-snap-on-input).
    @ObservationIgnored
    public var onBeforeSend: (() -> Void)?

    // MARK: - Private

    /// Connect timeout forwarded to Rust.
    public static let connectTimeoutMs: UInt64 = 5_000

    private var handle: OpaquePointer?

    // Two-stage pipeline:
    // internalStream ← sessionDataCallback (Rust read-loop thread)
    // pumpTask drains internalStream on MainActor → terminalCore + feedStream
    private let internalStream: AsyncStream<Data>
    private let internalContinuation: AsyncStream<Data>.Continuation
    private let internalBox: StreamContinuationBox

    private let feedStream: AsyncStream<Data>
    private let feedContinuation: AsyncStream<Data>.Continuation

    private let stateBox: StateCallbackBox
    private var pumpTask: Task<Void, Never>?

    private let ffiQueue = DispatchQueue(
        label: "com.bedterm.terminal-session",
        qos: .userInitiated
    )

    // MARK: - Init / Deinit

    public init() {
        var intCont: AsyncStream<Data>.Continuation!
        self.internalStream = AsyncStream<Data> { intCont = $0 }
        self.internalContinuation = intCont
        self.internalBox = StreamContinuationBox(intCont)

        var feedCont: AsyncStream<Data>.Continuation!
        self.feedStream = AsyncStream<Data> { feedCont = $0 }
        self.feedContinuation = feedCont

        self.terminalCore = TerminalCore(cols: 80, rows: 24)
        self.stateBox = StateCallbackBox()

        // `stateBox` is kept alive by `self`; use `passUnretained`.
        let stateCtx = Unmanaged.passUnretained(stateBox).toOpaque()
        guard let ptr = bt_terminal_session_create(sessionStateC, stateCtx) else {
            fatalError("RustTerminalSession: bt_terminal_session_create returned NULL")
        }
        self.handle = ptr

        // Install mock script override if set (UI-test injection point).
        // Must happen before any connect call so Rust sees the override in time.
        if let mockScript = RustTerminalSessionMockOverride.scriptName {
            mockScript.withCString { scriptPtr in
                bt_terminal_session_install_mock(ptr, scriptPtr)
            }
        }

        let sinkCtx = Unmanaged.passUnretained(internalBox).toOpaque()
        bt_terminal_session_set_data_sink(ptr, sessionDataCallback, sinkCtx)

        // Wire the closure after `self` is fully initialised.
        self.stateBox.onState = { [weak self] rustState, errorCode, exitCode in
            Task { @MainActor [weak self] in
                self?.onStateChange(rustState, errorCode: errorCode, exitCode: exitCode)
            }
        }
    }

    deinit {
        ffiQueue.sync {
            if let ptr = handle {
                handle = nil
                bt_terminal_session_close(ptr)
            }
        }
        internalContinuation.finish()
        feedContinuation.finish()
    }

    // MARK: - Connect

    /// Connect to the SSH server, open a PTY shell, and optionally send a
    /// bootstrap payload. Returns after the connect attempt completes.
    public func connect(
        credential: HostCredential,
        initialPTY: PTYDimensions,
        bootstrapPayload: String? = nil,
        timeout: TimeInterval = Double(RustTerminalSession.connectTimeoutMs) / 1_000.0
    ) async {
        NSLog(
            "[bedterm-diag] RustTerminalSession.connect initialPTY=%dx%d",
            initialPTY.cols, initialPTY.rows
        )
        state = .connecting
        lastError = nil

        let credJson: String
        do {
            credJson = try sshCredentialJson(credential.auth)
        } catch let err as SSHError {
            lastError = err
            state = .closed(reason: Self.describe(err))
            return
        } catch {
            state = .closed(reason: String(describing: error))
            return
        }

        do {
            try await callConnect(
                credential: credential,
                credJson: credJson,
                initialPTY: initialPTY,
                bootstrapPayload: bootstrapPayload,
                timeoutMs: UInt64(timeout * 1_000.0)
            )
            state = .open
            startPumpTask()
        } catch let err as SSHError {
            lastError = err
            state = .closed(reason: Self.describe(err))
        } catch {
            state = .closed(reason: String(describing: error))
        }
    }

    // MARK: - Send / Resize

    /// Write `data` to the remote PTY. Fire-and-forget; errors are dropped.
    public func send(_ data: Data) {
        guard case .open = state else { return }
        onBeforeSend?()
        guard let ptr = handle, !data.isEmpty else { return }
        ffiQueue.async { [data] in
            data.withUnsafeBytes { rawBuf in
                guard let base = rawBuf.baseAddress?.assumingMemoryBound(to: UInt8.self)
                else { return }
                bt_terminal_session_send(ptr, base, UInt(rawBuf.count), nil, nil)
            }
        }
    }

    /// Notify the remote PTY of a viewport resize.
    public func resize(cols: Int, rows: Int) {
        guard case .open = state else { return }
        guard let ptr = handle else { return }
        let newCols = UInt16(clamping: cols)
        let newRows = UInt16(clamping: rows)
        ffiQueue.async {
            bt_terminal_session_resize(ptr, newCols, newRows, nil, nil)
        }
    }

    // MARK: - Disconnect

    /// Initiate a graceful disconnect and mark the session closed.
    public func disconnect() {
        pumpTask?.cancel()
        if let ptr = handle {
            ffiQueue.async {
                bt_terminal_session_disconnect(ptr)
            }
        }
        internalContinuation.finish()
        feedContinuation.finish()
        state = .closed(reason: String(localized: "Closed"))
    }

    // MARK: - Error description

    nonisolated public static func describe(_ error: SSHError) -> String {
        describeSimple(error) ?? describeWithDetails(error)
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
                localized:
                    "Connection reset by the remote host (network change or idle timeout). Tap to reconnect."
            )
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

    // MARK: - Private helpers

    /// Blocking FFI call dispatched to `ffiQueue`; bridged to an async
    /// throwing continuation. Extracted from `connect` to respect the
    /// function-body-length limit.
    private func callConnect(
        credential: HostCredential,
        credJson: String,
        initialPTY: PTYDimensions,
        bootstrapPayload: String?,
        timeoutMs: UInt64
    ) async throws {
        let host = credential.host
        let username = credential.username
        let port = UInt16(clamping: credential.port)
        let cols = UInt16(clamping: initialPTY.cols)
        let rows = UInt16(clamping: initialPTY.rows)
        let bootstrap = bootstrapPayload

        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            ffiQueue.async { [self] in
                guard let ptr = self.handle else {
                    cont.resume(throwing: SSHError.disconnected("client closed"))
                    return
                }
                let resultBox = SyncResultBox()
                let resultCtx = Unmanaged.passUnretained(resultBox).toOpaque()
                // Inline withCString nesting so UnsafePointer values stay within
                // the same closure scope (avoiding Swift 6 Sendable errors).
                host.withCString { hostPtr in
                    username.withCString { userPtr in
                        credJson.withCString { credPtr in
                            if let bs = bootstrap, !bs.isEmpty {
                                bs.withCString { bsPtr in
                                    bt_terminal_session_connect(
                                        ptr, hostPtr, port, userPtr, credPtr, bsPtr,
                                        cols, rows, timeoutMs,
                                        sessionCompletionCallback, resultCtx
                                    )
                                }
                            } else {
                                bt_terminal_session_connect(
                                    ptr, hostPtr, port, userPtr, credPtr, nil,
                                    cols, rows, timeoutMs,
                                    sessionCompletionCallback, resultCtx
                                )
                            }
                        }
                    }
                }
                if let result = resultBox.result {
                    cont.resume(with: result)
                } else {
                    cont.resume(throwing: SSHError.disconnected("connect returned without callback"))
                }
            }
        }
    }

    private func startPumpTask() {
        pumpTask = Task { @MainActor [weak self] in
            guard let self else { return }
            for await chunk in self.internalStream {
                self.terminalCore.feed(chunk)
                let newMode = self.terminalCore.mode
                if newMode != self.mode { self.mode = newMode }
                self.feedContinuation.yield(chunk)
            }
            if case .open = self.state {
                self.state = .closed(reason: String(localized: "Connection ended"))
            }
            self.feedContinuation.finish()
        }
    }

    /// Handles Rust state transitions delivered after a successful connect
    /// (i.e. read-loop EOF or remote disconnect). The `connect` method
    /// handles the initial Open/Closed transition itself.
    @MainActor
    fileprivate func onStateChange(
        _ rustState: BtSessionState,
        errorCode: BtSSHResultCode,
        exitCode: Int32
    ) {
        guard rustState == Closed, case .open = self.state else { return }
        let err = errorCode == BtSSHResultOk ? nil : sessionSshError(from: errorCode, extra: exitCode)
        lastError = err
        let reason = err.map { Self.describe($0) } ?? String(localized: "Connection ended")
        state = .closed(reason: reason)
        internalContinuation.finish()
        // feedContinuation is finished by pumpTask when internalStream drains.
    }
}
