import Foundation

/// Errors thrown by Keychain-backed stores (`HostsStore`, `HostKeyStore`).
///
/// The underlying Keychain access lives in Rust
/// (`rust-core/bedterm-ios/src/{hosts_store,host_key_store}.rs`). The
/// Swift wrappers raise these cases to keep the throwing API stable
/// for call sites that pre-date the Rust sink (notably the TOFU host-key
/// validator's `catch KeychainError.notFound`).
///
/// - `.notFound` — the FFI call returned NULL / "no item" (Rust treats
///   `errSecItemNotFound` as a normal absence signal).
/// - `.status(OSStatus)` — the FFI call reported a write failure. The
///   OSStatus payload is zero today because the C ABI just returns a
///   `bool`; it's retained for source compatibility.
enum KeychainError: Error, Equatable {
    case notFound
    case status(OSStatus)
}
