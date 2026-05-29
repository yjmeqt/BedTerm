import BedTermIOS
import Foundation

// MARK: - Callback boxes

/// Heap box so the state-change callback (which fires on an arbitrary
/// thread) can safely dispatch to the MainActor without retain-cycling
/// the session.
final class StateCallbackBox: @unchecked Sendable {
    /// Closure installed by `init` after the Rust handle exists.
    /// Called with the raw Rust state values; dispatches to the main actor.
    var onState: ((BtSessionState, BtSSHResultCode, Int32) -> Void)?
}

/// Heap box wrapping the internal inbound stream continuation.
final class StreamContinuationBox: @unchecked Sendable {
    let continuation: AsyncStream<Data>.Continuation
    init(_ cont: AsyncStream<Data>.Continuation) { self.continuation = cont }
}

/// Sync-result box for bridging C completions to throwing Swift callers.
final class SyncResultBox {
    var result: Result<Void, Error>?
}

// MARK: - File-scope C callbacks (captureless, @convention(c))

/// Shared completion callback for `bt_terminal_session_connect`,
/// `bt_terminal_session_send`, and `bt_terminal_session_resize`.
/// Fires synchronously on the calling thread.
let sessionCompletionCallback: BtSSHCompletion = { ctx, code, _, extra in
    guard let ctx else { return }
    let box = Unmanaged<SyncResultBox>.fromOpaque(ctx).takeUnretainedValue()
    if code == BtSSHResultOk {
        box.result = .success(())
    } else {
        box.result = .failure(sessionSshError(from: code, extra: extra))
    }
}

/// Data sink for inbound PTY bytes. Fires on the Rust read-loop thread.
let sessionDataCallback: BtSSHOutputSink = { ctx, bytes, len in
    guard let ctx, let bytes, len > 0 else { return }
    let box = Unmanaged<StreamContinuationBox>.fromOpaque(ctx).takeUnretainedValue()
    box.continuation.yield(Data(bytes: bytes, count: Int(len)))
}

/// State-change callback registered at `bt_terminal_session_create` time.
/// Fires on an arbitrary Rust thread; delegates to `StateCallbackBox.onState`.
let sessionStateC:
    @convention(c) (
        UnsafeMutableRawPointer?, BtSessionState, BtSSHResultCode, Int32
    ) -> Void = { ctx, rustState, errorCode, exitCode in
        guard let ctx else { return }
        let box = Unmanaged<StateCallbackBox>.fromOpaque(ctx).takeUnretainedValue()
        box.onState?(rustState, errorCode, exitCode)
    }

// MARK: - BtSSHResultCode → SSHError

func sessionSshError(from code: BtSSHResultCode, extra: Int32) -> SSHError {
    if code == BtSSHResultShellExited { return .shellExited(Int(extra)) }
    return sessionSimpleSshError(code) ?? .disconnected("SSH error \(code.rawValue)")
}

func sessionSimpleSshError(_ code: BtSSHResultCode) -> SSHError? {
    switch code {
    case BtSSHResultDnsResolution: return .dnsResolution
    case BtSSHResultTcpRefused: return .tcpRefused
    case BtSSHResultTimeout: return .timeout
    case BtSSHResultHandshakeFailed: return .handshakeFailed("")
    case BtSSHResultAuthenticationFailed: return .authenticationFailed
    case BtSSHResultPrivateKeyParse: return .privateKeyParse
    case BtSSHResultPrivateKeyPassphraseRequired: return .privateKeyPassphraseRequired
    case BtSSHResultHostKeyMismatch: return .hostKeyMismatch(stored: "", remote: "")
    case BtSSHResultDisconnected: return .disconnected("")
    case BtSSHResultPeerReset: return .peerReset
    default: return nil
    }
}

// MARK: - Credential JSON helpers

func sshCredentialJson(_ auth: HostCredential.AuthMethod) throws -> String {
    switch auth {
    case .password(let pw):
        return "{\"type\":\"password\",\"password\":\(jsonEscapeString(pw))}"
    case .privateKey(let keyData, let passphrase):
        guard let keyString = String(data: keyData, encoding: .utf8) else {
            throw SSHError.privateKeyParse
        }
        if let pp = passphrase {
            return
                "{\"type\":\"private_key\",\"private_key\":\(jsonEscapeString(keyString)),\"passphrase\":\(jsonEscapeString(pp))}"
        }
        return "{\"type\":\"private_key\",\"private_key\":\(jsonEscapeString(keyString))}"
    }
}

func jsonEscapeString(_ str: String) -> String {
    var out = "\""
    for byte in str.utf8 {
        switch byte {
        case UInt8(ascii: "\\"): out += "\\\\"
        case UInt8(ascii: "\""): out += "\\\""
        case UInt8(ascii: "\n"): out += "\\n"
        case UInt8(ascii: "\r"): out += "\\r"
        case UInt8(ascii: "\t"): out += "\\t"
        case 0x00...0x1F: out += "\\u00\(String(byte, radix: 16, uppercase: true))"
        default: out += String(UnicodeScalar(byte))
        }
    }
    return out + "\""
}
