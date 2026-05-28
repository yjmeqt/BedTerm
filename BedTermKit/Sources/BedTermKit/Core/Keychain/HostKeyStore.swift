import BedTermIOS
import Foundation

/// Per-host SSH host-key fingerprint store.
///
/// Storage lives in Rust (`rust-core/bedterm-ios/src/host_key_store.rs`)
/// — per-host Keychain blobs keyed under service
/// `com.applovin.yi.bedterm.hostkeys`, account `"{host}:{port}"`,
/// value = UTF-8 fingerprint string. This Swift wrapper round-trips
/// the public API through the `bt_ios_host_keys_*` C ABI; the TOFU
/// validator in `CitadelSSHClient` continues to call the same methods
/// against the same `Verdict` enum.
///
/// The `service` initialiser parameter is a no-op in production — the
/// Rust singleton always uses the canonical service string. Tests
/// install an in-memory backend via
/// `bt_ios_host_keys_set_test_service(...)` before calling any store
/// method.
public struct HostKeyStore: Sendable {
    public enum Verdict: Equatable {
        case match
        case mismatch(stored: String, remote: String)
        case unknown
    }

    public init(service: String = "com.applovin.yi.bedterm.hostkeys") {
        // The service argument is retained only for source compatibility
        // with the previous Swift implementation. Rust owns the canonical
        // service string; tests route via `bt_ios_host_keys_set_test_service`.
        _ = service
    }

    public func fingerprint(host: String, port: Int) throws -> String? {
        guard
            let cStr = host.withCString({ hostPtr in
                bt_ios_host_keys_load(hostPtr, UInt16(port))
            })
        else {
            return nil
        }
        defer { bt_ios_host_keys_free_string(cStr) }
        return String(cString: cStr)
    }

    public func store(fingerprint: String, host: String, port: Int) throws {
        let ok = host.withCString { hostPtr in
            fingerprint.withCString { fpPtr in
                bt_ios_host_keys_save(hostPtr, UInt16(port), fpPtr)
            }
        }
        if !ok {
            throw KeychainError.status(0)
        }
    }

    /// Drop the stored fingerprint for `host:port`, if any. No-op when
    /// none is stored.
    public func remove(host: String, port: Int) {
        host.withCString { hostPtr in
            bt_ios_host_keys_delete(hostPtr, UInt16(port))
        }
    }

    public func verify(remote: String, host: String, port: Int) throws -> Verdict {
        var outStored: UnsafeMutablePointer<CChar>?
        let discriminant = host.withCString { hostPtr in
            remote.withCString { remotePtr in
                bt_ios_host_keys_verify(hostPtr, UInt16(port), remotePtr, &outStored)
            }
        }
        switch discriminant {
        case 0:
            return .match
        case 1:
            guard let outStored else { return .unknown }
            defer { bt_ios_host_keys_free_string(outStored) }
            let storedString = String(cString: outStored)
            return .mismatch(stored: storedString, remote: remote)
        default:
            return .unknown
        }
    }
}
