import BedTermCoreC
import Foundation

/// Swift→Rust bridge for the W24b Rust Hosts list VC.
///
/// The Rust `BtIosHostsListViewController` renders rows but doesn't own
/// connect / delete orchestration — that stays in Swift's existing
/// `HostsViewModel`. This bridge gives the Rust side three primitives:
///
/// - Snapshot: pull the current `[SavedHost]` as a UTF-8 JSON blob.
/// - Delete:   remove a host by id (delegates to `HostsStore`).
/// - Connect:  dispatch a connect attempt for a host id (delegates to
///             the `connectHandler` installed by `RootCoordinator`).
///
/// All `@_cdecl` accessors below are main-actor isolated and reach into
/// the shared store / handler via `MainActor.assumeIsolated` because the
/// Rust VC calls them from selector handlers (always on the main thread).
@MainActor
public enum HostsBridge {
    /// Backing `HostsStore` the snapshot / delete shims use. Defaults to
    /// a fresh `HostsStore()`; tests can swap it out before invoking the
    /// `@_cdecl` symbols.
    public static var store = HostsStore()

    /// Optional entry provider — when set, `snapshotJSON()` reads from
    /// here instead of `store.list()`. Installed by
    /// `HostsConnectController` so the Rust VC sees the same entry set
    /// the Swift `HostsViewModel` does (including UI-test stub-host
    /// injection via `HostsStoreInjection`).
    public static var entriesProvider: (() -> [SavedHost])?

    /// Closure the `bt_swift_hosts_connect` shim invokes with the parsed
    /// UUID. Installed by `RootCoordinator` to forward into the active
    /// `HostsViewModel.requestConnect`. Left nil for unit tests.
    public static var connectHandler: ((UUID) -> Void)?

    /// Closure the `bt_swift_hosts_add` path uses (currently fired
    /// directly via the C `on_add` callback from the VC constructor —
    /// kept here for symmetry / future expansion).
    public static var addHandler: (() -> Void)?

    /// Render the current `[SavedHost]` array as a JSON blob the Rust
    /// `parse_entries_json` helper understands. Returns "[]" when the
    /// store is empty or the device is locked.
    ///
    /// When `entriesProvider` is installed (UI tests + the live hosts
    /// screen so injected stub hosts surface), the JSON is built in
    /// Swift so the array can include in-memory rows that never reach
    /// the Keychain. Otherwise this delegates to Rust's
    /// `bt_ios_hosts_snapshot_json`, which is the production fast path.
    public static func snapshotJSON() -> String {
        if let provider = self.entriesProvider {
            return Self.buildSnapshotJSON(from: provider())
        }
        guard let cStr = bt_ios_hosts_snapshot_json() else { return "[]" }
        defer { bt_ios_hosts_free_string(cStr) }
        return String(cString: cStr)
    }

    private static func buildSnapshotJSON(from entries: [SavedHost]) -> String {
        var items: [[String: Any]] = []
        items.reserveCapacity(entries.count)
        for entry in entries {
            let cred = entry.credential
            let authIsKey: Bool
            switch cred.auth {
            case .privateKey: authIsKey = true
            case .password: authIsKey = false
            }
            items.append([
                "id": entry.id.uuidString,
                "label": entry.label,
                "host": cred.host,
                "port": cred.port,
                "username": cred.username,
                "authIsKey": authIsKey
            ])
        }
        guard let data = try? JSONSerialization.data(withJSONObject: items, options: []),
            let json = String(data: data, encoding: .utf8)
        else {
            return "[]"
        }
        return json
    }
}

// MARK: - C ABI
//
// The function names match the symbol patterns the Rust hosts VC's
// `extern "C"` declarations reach for via `bt_swift_hosts_*`. Keep both
// sides in sync — adding a new accessor here requires adding the
// matching extern in `rust-core/bedterm_ios/src/hosts/bridge.rs`.

@_cdecl("bt_swift_hosts_snapshot_json")
public func btSwiftHostsSnapshotJSON() -> UnsafeMutablePointer<CChar>? {
    let json = MainActor.assumeIsolated { HostsBridge.snapshotJSON() }
    // strdup → +1 retained C buffer. Rust frees via
    // `bt_swift_hosts_free_snapshot`.
    return json.withCString { strdup($0) }
}

@_cdecl("bt_swift_hosts_free_snapshot")
public func btSwiftHostsFreeSnapshot(_ ptr: UnsafeMutablePointer<CChar>?) {
    guard let ptr else { return }
    free(ptr)
}

@_cdecl("bt_swift_hosts_delete")
public func btSwiftHostsDelete(_ idPtr: UnsafePointer<CChar>?) -> Bool {
    guard let idPtr else { return false }
    let idString = String(cString: idPtr)
    guard let uuid = UUID(uuidString: idString) else { return false }
    MainActor.assumeIsolated {
        HostsBridge.store.delete(id: uuid)
    }
    return true
}

@_cdecl("bt_swift_hosts_connect")
public func btSwiftHostsConnect(_ idPtr: UnsafePointer<CChar>?) {
    guard let idPtr else { return }
    let idString = String(cString: idPtr)
    guard let uuid = UUID(uuidString: idString) else { return }
    MainActor.assumeIsolated {
        HostsBridge.connectHandler?(uuid)
    }
}
