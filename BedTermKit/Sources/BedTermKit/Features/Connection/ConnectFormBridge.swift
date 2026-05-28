import BedTermIOS
import Foundation
import UIKit
import UniformTypeIdentifiers

/// Swift->Rust bridge for the W24c Rust connect-form VC.
///
/// After the W24e refactor the Rust `BtIosConnectFormViewController` calls
/// `connect_form_vm::VM` directly for all state + validation. The only
/// functions that still cross FFI to Swift are:
///
/// - **File picker** — `bt_swift_connect_form_pick_key` (presents
///   `UIDocumentPickerViewController`), plus the take/free key-bytes trio.
/// - **Persistence** — `bt_swift_hosts_store_save_json` deserialises the
///   `SaveOutcome` JSON and persists the entry via `bt_ios_hosts_save_blob`.
///
/// All accessors are main-actor isolated — the Rust VC calls them on the
/// main thread (selector handlers).
@MainActor
public enum ConnectFormBridge {
    /// Pending key bytes from the most recent document-picker round-trip.
    /// Set by the picker delegate; consumed by the Rust VC's `try_save`
    /// via `bt_swift_connect_form_take_pending_key_bytes`. Cleared on take.
    public static var pendingKeyBytes: Data?

    /// Human-readable label for the pending key file (e.g. `id_ed25519`).
    /// Surfaced to Rust on the picker callback so the "Choose key..." row
    /// can render the picked file name.
    public static var pendingKeyLabel: String?

    /// Retained delegate for the most recent in-flight document picker.
    /// Mirrors the trampoline-retention pattern in
    /// `RootCoordinator+RustOnboarding.swift`: the system picker holds a
    /// weak ref to its delegate, so we pin it here for the lifetime of
    /// the presentation. Replaced on the next pick (the previous picker
    /// has already dismissed by then).
    fileprivate static var currentPickDelegate: KeyPickDelegate?
}

// MARK: - C ABI

@_cdecl("bt_swift_connect_form_pick_key")
public func btSwiftConnectFormPickKey(
    _ callback: @escaping @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> Void,
    _ ctx: UnsafeMutableRawPointer?
) {
    let trampoline = PickKeyTrampoline(callback: callback, ctx: ctx)
    MainActor.assumeIsolated {
        guard let presenter = ConnectFormBridge.topmostViewController() else {
            trampoline.fire(label: nil)
            return
        }
        let delegate = KeyPickDelegate(trampoline: trampoline)
        ConnectFormBridge.currentPickDelegate = delegate
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.data])
        picker.delegate = delegate
        picker.allowsMultipleSelection = false
        picker.shouldShowFileExtensions = true
        presenter.present(picker, animated: true)
    }
}

/// Sendable wrapper that hides the non-Sendable C callback + raw `ctx`
/// from Swift's strict-concurrency checker.
final class PickKeyTrampoline: @unchecked Sendable {
    private let callback: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> Void
    private let ctxBits: UInt
    private var didFire = false

    init(
        callback: @escaping @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> Void,
        ctx: UnsafeMutableRawPointer?
    ) {
        self.callback = callback
        self.ctxBits = ctx.map { UInt(bitPattern: $0) } ?? 0
    }

    func fire(label: String?) {
        guard !self.didFire else { return }
        self.didFire = true
        let ctx =
            self.ctxBits == 0
            ? nil
            : UnsafeMutableRawPointer(bitPattern: self.ctxBits)
        if let label {
            label.withCString { cstr in
                self.callback(ctx, cstr)
            }
        } else {
            self.callback(ctx, nil)
        }
    }
}

@_cdecl("bt_swift_connect_form_take_pending_key_bytes")
public func btSwiftConnectFormTakePendingKeyBytes(
    _ outLen: UnsafeMutablePointer<Int>?
) -> UnsafeMutablePointer<UInt8>? {
    let payload: Data? = MainActor.assumeIsolated {
        let bytes = ConnectFormBridge.pendingKeyBytes
        ConnectFormBridge.pendingKeyBytes = nil
        ConnectFormBridge.pendingKeyLabel = nil
        return bytes
    }
    guard let data = payload, !data.isEmpty else {
        outLen?.pointee = 0
        return nil
    }
    let count = data.count
    let buf = UnsafeMutablePointer<UInt8>.allocate(capacity: count)
    data.withUnsafeBytes { raw in
        if let base = raw.baseAddress {
            buf.initialize(from: base.assumingMemoryBound(to: UInt8.self), count: count)
        }
    }
    outLen?.pointee = count
    return buf
}

@_cdecl("bt_swift_connect_form_free_key_bytes")
public func btSwiftConnectFormFreeKeyBytes(_ ptr: UnsafeMutablePointer<UInt8>?) {
    guard let ptr else { return }
    ptr.deallocate()
}

/// Deserialise a JSON `SaveOutcome` from the Rust VM and persist it
/// directly through the Rust Keychain FFI (`bt_ios_hosts_save_blob`).
@_cdecl("bt_swift_hosts_store_save_json")
public func btSwiftHostsStoreSaveJson(_ jsonPtr: UnsafePointer<CChar>?) {
    guard let jsonPtr else { return }
    let json = String(cString: jsonPtr)
    MainActor.assumeIsolated {
        guard let data = json.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let idString = obj["id"] as? String,
            let uuid = UUID(uuidString: idString),
            let hostStr = obj["host"] as? String,
            let port = obj["port"] as? Int,
            let username = obj["username"] as? String,
            let authIsKey = obj["authIsKey"] as? Bool
        else { return }
        let label = (obj["label"] as? String) ?? ""
        let auth: HostCredential.AuthMethod
        if authIsKey {
            guard let keyB64 = obj["privateKeyBase64"] as? String,
                let keyData = Data(base64Encoded: keyB64)
            else { return }
            let passphrase = obj["passphrase"] as? String
            auth = .privateKey(keyData, passphrase: passphrase)
        } else {
            let password = (obj["password"] as? String) ?? ""
            auth = .password(password)
        }
        let credential = HostCredential(host: hostStr, port: port, username: username, auth: auth)
        let entry = SavedHost(id: uuid, label: label, credential: credential)
        if let data = try? JSONEncoder().encode(entry) {
            _ = entry.id.uuidString.withCString { idPtr in
                data.withUnsafeBytes { raw -> Bool in
                    let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                    return bt_ios_hosts_save_blob(idPtr, base, UInt(raw.count))
                }
            }
        }
    }
}

// MARK: - Picker plumbing

extension ConnectFormBridge {
    /// Walks the active foreground scene's key window up through any
    /// `presentedViewController` chain to find the VC we should call
    /// `present(_:animated:)` on.
    static func topmostViewController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes
        let active =
            scenes
            .compactMap { $0 as? UIWindowScene }
            .first(where: { $0.activationState == .foregroundActive })
            ?? scenes.compactMap({ $0 as? UIWindowScene }).first
        let window = active?.windows.first(where: \.isKeyWindow) ?? active?.windows.first
        var top = window?.rootViewController
        while let presented = top?.presentedViewController {
            top = presented
        }
        return top
    }
}

/// Retained delegate for the in-flight `UIDocumentPickerViewController`.
/// Reads the picked file's bytes under `startAccessingSecurityScopedResource`,
/// stashes them on `ConnectFormBridge.pendingKey{Bytes,Label}`, then fires
/// the C callback exactly once.
final class KeyPickDelegate: NSObject, UIDocumentPickerDelegate {
    private let trampoline: PickKeyTrampoline

    init(trampoline: PickKeyTrampoline) {
        self.trampoline = trampoline
    }

    private func clearRetain() {
        MainActor.assumeIsolated {
            if ConnectFormBridge.currentPickDelegate === self {
                ConnectFormBridge.currentPickDelegate = nil
            }
        }
    }

    func documentPicker(
        _ controller: UIDocumentPickerViewController,
        didPickDocumentsAt urls: [URL]
    ) {
        guard let url = urls.first else {
            self.trampoline.fire(label: nil)
            self.clearRetain()
            return
        }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let bytes = try Data(contentsOf: url)
            let label = url.lastPathComponent
            MainActor.assumeIsolated {
                ConnectFormBridge.pendingKeyBytes = bytes
                ConnectFormBridge.pendingKeyLabel = label
            }
            self.trampoline.fire(label: label)
        } catch {
            self.trampoline.fire(label: nil)
        }
        self.clearRetain()
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        self.trampoline.fire(label: nil)
        self.clearRetain()
    }
}
