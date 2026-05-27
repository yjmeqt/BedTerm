import Foundation

/// Swift->Rust bridge for the W24b Rust Hosts list VC.
///
/// The Rust `BtIosHostsListViewController` renders rows but doesn't own
/// connect / delete orchestration — that stays in Swift's existing
/// `HostsViewModel`. This bridge gives the Rust side one primitive:
///
/// - Connect:  dispatch a connect attempt for a host id (delegates to
///             the `connectHandler` installed by `RootCoordinator`).
///
/// The snapshot and delete primitives formerly carried by
/// `bt_swift_hosts_snapshot_json` / `bt_swift_hosts_free_snapshot` /
/// `bt_swift_hosts_delete` were removed in favour of direct
/// `crate::hosts_store` calls from the Rust VC — cutting the
/// Rust->Swift->Rust round trip for those operations.
///
/// The `@_cdecl` accessor below is main-actor isolated and reaches into
/// the shared handler via `MainActor.assumeIsolated` because the Rust VC
/// calls it from selector handlers (always on the main thread).
@MainActor
public enum HostsBridge {
    /// Closure the `bt_swift_hosts_connect` shim invokes with the parsed
    /// UUID. Installed by `RootCoordinator` to forward into the active
    /// `HostsViewModel.requestConnect`. Left nil for unit tests.
    public static var connectHandler: ((UUID) -> Void)?
}

// MARK: - C ABI
//
// The function name matches the symbol pattern the Rust hosts VC's
// `extern "C"` declaration reaches for via `bt_swift_hosts_connect`.

@_cdecl("bt_swift_hosts_connect")
public func btSwiftHostsConnect(_ idPtr: UnsafePointer<CChar>?) {
    guard let idPtr else { return }
    let idString = String(cString: idPtr)
    guard let uuid = UUID(uuidString: idString) else { return }
    MainActor.assumeIsolated {
        HostsBridge.connectHandler?(uuid)
    }
}
