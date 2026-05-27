import Foundation
import UIKit
import UniformTypeIdentifiers

/// Swift→Rust bridge for the W24c Rust connect-form VC.
///
/// The Rust `BtIosConnectFormViewController` collects + validates input,
/// then asks Swift to persist via the `bt_swift_connect_form_*` `@_cdecl`
/// shims below. Persistence reuses `ConnectionFormViewModel.save` so the
/// SwiftUI form and the Rust VC share one keychain-write path.
///
/// All accessors are main-actor isolated — the Rust VC calls them on the
/// main thread (selector handlers).
@MainActor
public enum ConnectFormBridge {
    /// Backing `HostsStore` the prefill / save shims use. Defaults to a
    /// fresh `HostsStore()`; tests can swap it out before invoking the
    /// `@_cdecl` symbols.
    public static var store = HostsStore()

    /// Last validation error message (from the most recent save attempt
    /// that failed). Cleared on a successful save. Rust pulls this via
    /// `bt_swift_connect_form_last_error` and surfaces it inline.
    public static var lastError: String?

    /// Pending key bytes from the most recent document-picker round-trip.
    /// Set by the picker delegate; consumed by `save(...)` and cleared
    /// regardless of outcome. Also reachable by Rust via
    /// `bt_swift_connect_form_take_pending_key_bytes` for tests + future
    /// validation pre-save.
    public static var pendingKeyBytes: Data?

    /// Human-readable label for the pending key file (e.g. `id_ed25519`).
    /// Surfaced to Rust on the picker callback so the "Choose key…" row
    /// can render the picked file name.
    public static var pendingKeyLabel: String?

    /// Retained delegate for the most recent in-flight document picker.
    /// Mirrors the trampoline-retention pattern in
    /// `RootCoordinator+RustOnboarding.swift`: the system picker holds a
    /// weak ref to its delegate, so we pin it here for the lifetime of
    /// the presentation. Replaced on the next pick (the previous picker
    /// has already dismissed by then).
    fileprivate static var currentPickDelegate: KeyPickDelegate?

    /// Mirror of `ConnectionFormViewModel`'s edit-mode snapshot used by
    /// the bridge prefill path. JSON-encoded; consumed by Rust's
    /// `apply_prefill`.
    static func prefillJSON(for id: UUID) -> String? {
        guard let entry = try? store.load(id: id) else { return nil }
        var dict: [String: Any] = [
            "id": entry.id.uuidString,
            "label": entry.label,
            "host": entry.credential.host,
            "port": String(entry.credential.port),
            "username": entry.credential.username
        ]
        switch entry.credential.auth {
        case .password:
            dict["authIsKey"] = false
            dict["passwordSet"] = true
            dict["keySet"] = false
        case .privateKey:
            dict["authIsKey"] = true
            dict["passwordSet"] = false
            dict["keySet"] = true
        }
        guard let data = try? JSONSerialization.data(withJSONObject: dict, options: []),
            let json = String(data: data, encoding: .utf8)
        else {
            return nil
        }
        return json
    }

    /// Persist the draft. Returns the saved entry's id on success, or
    /// throws `ConnectionFormViewModel.FormError.validation` on failure.
    /// Reuses the SwiftUI view-model so validation + Keychain semantics
    /// stay identical across the two front ends.
    @discardableResult
    static func save(
        draftJSON: String,
        newPassword: String?,
        passphrase: String?
    ) throws -> UUID {
        guard
            let data = draftJSON.data(using: .utf8),
            let dict = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            throw ConnectionFormViewModel.FormError.validation(
                String(localized: "Internal error decoding form data.")
            )
        }
        let editingID: UUID? = (dict["id"] as? String).flatMap { $0.isEmpty ? nil : UUID(uuidString: $0) }
        let mode: ConnectionFormViewModel.Mode
        if let id = editingID, let existing = try? store.load(id: id) {
            mode = .edit(existing)
        } else {
            mode = .add
        }
        let vm = ConnectionFormViewModel(mode: mode, store: store)
        vm.label = (dict["label"] as? String) ?? ""
        vm.host = (dict["host"] as? String) ?? ""
        vm.port = (dict["port"] as? String) ?? "22"
        vm.username = (dict["username"] as? String) ?? ""
        let authMode = (dict["authMode"] as? String) ?? "password"
        vm.auth = (authMode == "key") ? .privateKey : .password
        if let pw = newPassword, !pw.isEmpty {
            vm.password = pw
            vm.passwordTouched = true
        }
        if let pass = passphrase, !pass.isEmpty {
            vm.passphrase = pass
            vm.passphraseTouched = true
        }
        if let keyData = pendingKeyBytes {
            vm.privateKey = keyData
            vm.privateKeyTouched = true
        }
        defer { pendingKeyBytes = nil }
        return try vm.save()
    }
}

// MARK: - C ABI

@_cdecl("bt_swift_connect_form_prefill_json")
public func btSwiftConnectFormPrefillJSON(
    _ idPtr: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>? {
    guard let idPtr else { return nil }
    let idString = String(cString: idPtr)
    guard let uuid = UUID(uuidString: idString) else { return nil }
    let json = MainActor.assumeIsolated { ConnectFormBridge.prefillJSON(for: uuid) }
    guard let json else { return nil }
    return json.withCString { strdup($0) }
}

@_cdecl("bt_swift_connect_form_free_snapshot")
public func btSwiftConnectFormFreeSnapshot(_ ptr: UnsafeMutablePointer<CChar>?) {
    guard let ptr else { return }
    free(ptr)
}

@_cdecl("bt_swift_connect_form_save")
public func btSwiftConnectFormSave(
    _ draftPtr: UnsafePointer<CChar>?,
    _ passwordPtr: UnsafePointer<CChar>?,
    _ passphrasePtr: UnsafePointer<CChar>?,
    _ outID: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Bool {
    guard let draftPtr else { return false }
    let draftJSON = String(cString: draftPtr)
    let newPassword = passwordPtr.map { String(cString: $0) }
    let passphrase = passphrasePtr.map { String(cString: $0) }
    // `outID` is `UnsafeMutablePointer<…>?` which isn't Sendable; capture
    // its raw bit-pattern instead so the @MainActor closure has nothing
    // non-Sendable to send across.
    let outIDRaw: UInt = outID.map { UInt(bitPattern: $0) } ?? 0
    return MainActor.assumeIsolated {
        do {
            let id = try ConnectFormBridge.save(
                draftJSON: draftJSON,
                newPassword: newPassword,
                passphrase: passphrase
            )
            ConnectFormBridge.lastError = nil
            if outIDRaw != 0 {
                let restored = UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>(
                    bitPattern: outIDRaw)
                restored?.pointee = id.uuidString.withCString { strdup($0) }
            }
            return true
        } catch let ConnectionFormViewModel.FormError.validation(message) {
            ConnectFormBridge.lastError = message
            return false
        } catch {
            ConnectFormBridge.lastError = String(localized: "Could not save host.")
            return false
        }
    }
}

@_cdecl("bt_swift_connect_form_last_error")
public func btSwiftConnectFormLastError() -> UnsafeMutablePointer<CChar>? {
    let message = MainActor.assumeIsolated { ConnectFormBridge.lastError }
    guard let message else { return nil }
    return message.withCString { strdup($0) }
}

// Picker design choice (documented for future maintainers):
//   `save(...)` reads `pendingKeyBytes` directly — the simpler shape —
//   instead of forcing Rust to pull bytes out via
//   `bt_swift_connect_form_take_pending_key_bytes` and feed them back
//   through `save`. The take/free pair still ships so tests can pin the
//   round-trip and future Rust-side validation can inspect bytes.
@_cdecl("bt_swift_connect_form_pick_key")
public func btSwiftConnectFormPickKey(
    _ callback: @escaping @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> Void,
    _ ctx: UnsafeMutableRawPointer?
) {
    // The Rust VC always invokes this from a main-thread selector, so
    // we're effectively on the main actor here. Bridge the C callback +
    // raw ctx into a Sendable trampoline so the @MainActor closure
    // doesn't have to send non-Sendable values across an isolation hop.
    let trampoline = PickKeyTrampoline(callback: callback, ctx: ctx)
    MainActor.assumeIsolated {
        guard let presenter = ConnectFormBridge.topmostViewController() else {
            // No host VC (unit-test / headless contexts) — preserve the
            // pre-W24c-fix cancel contract so the Rust VC and the
            // `pickKeyFiresCancel` test don't hang.
            trampoline.fire(label: nil)
            return
        }
        let delegate = KeyPickDelegate(trampoline: trampoline)
        ConnectFormBridge.currentPickDelegate = delegate
        // SSH private keys are unstructured text; `.data` matches the
        // SwiftUI form's `.fileImporter(allowedContentTypes: [.data])`.
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.data])
        picker.delegate = delegate
        picker.allowsMultipleSelection = false
        picker.shouldShowFileExtensions = true
        presenter.present(picker, animated: true)
    }
}

/// Sendable wrapper that hides the non-Sendable C callback + raw `ctx`
/// from Swift's strict-concurrency checker. The C callback is plain
/// function-pointer data and `ctx` is an opaque integer-sized handle —
/// both are safe to send between executors, but the type system can't
/// see that.
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
            MainActor.assumeIsolated {
                ConnectFormBridge.lastError = String(
                    localized: "Could not read the selected key file."
                )
            }
            self.trampoline.fire(label: nil)
        }
        self.clearRetain()
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        self.trampoline.fire(label: nil)
        self.clearRetain()
    }
}
