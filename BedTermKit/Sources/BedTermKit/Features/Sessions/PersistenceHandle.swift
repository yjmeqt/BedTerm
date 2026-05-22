import BedTermCoreC
import Foundation
import SwiftUI

/// Process-wide handle to the SQLite persistence layer. One per app
/// lifetime, initialized in BedTermKit setup before any `TerminalSession`
/// is created.
///
/// Thread safety: not thread-safe. All calls must come from the same
/// thread (typically `@MainActor`). The underlying rusqlite Connection
/// is `!Sync` so we do not attempt cross-thread sharing.
///
/// Implementation note: `bedterm_persistence_init` returns an
/// `OpaquePointer` in Swift (the C struct body is hidden; cbindgen
/// forward-declares it). This mirrors the `TerminalCore` / `BtTerm *`
/// pattern exactly.
@MainActor
public final class PersistenceHandle {
    /// OpaquePointer wraps the C `PersistenceHandle *` — the struct body is
    /// intentionally hidden by the Rust-generated header (opaque /
    /// forward-declared only), mirroring the `TerminalCore` / `BtTerm *` pattern.
    /// `nonisolated(unsafe)` lets `deinit` (which is nonisolated) call
    /// `bedterm_persistence_close` without a MainActor hop — safe because
    /// we hold exclusive ownership and never alias the pointer.
    nonisolated(unsafe) private let raw: OpaquePointer

    /// Open (or create) the SQLite database at `path`. Returns nil if the
    /// file can't be opened. On first creation, ensures the file is
    /// protected with `.completeUntilFirstUserAuthentication`.
    public static func open(at path: URL) -> PersistenceHandle? {
        let fm = FileManager.default
        let dir = path.deletingLastPathComponent()
        try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        if !fm.fileExists(atPath: path.path) {
            fm.createFile(
                atPath: path.path,
                contents: nil,
                attributes: [
                    .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication
                ]
            )
        }
        // bedterm_persistence_init returns OpaquePointer? in Swift because
        // the C `PersistenceHandle` struct is opaque (forward-declared only).
        // We extract the raw pointer before constructing self to keep
        // the withCString closure result non-optional.
        var maybeRaw: OpaquePointer?
        path.path.withCString { cstr in
            maybeRaw = bedterm_persistence_init(cstr)
        }
        guard let raw = maybeRaw else { return nil }
        return PersistenceHandle(raw: raw)
    }

    private init(raw: OpaquePointer) {
        self.raw = raw
    }

    deinit {
        bedterm_persistence_close(raw)
    }

    /// Internal-only — used by FFI call sites within BedTermKit.
    var unsafeHandle: OpaquePointer { raw }
}

// MARK: - SwiftUI environment key

private struct PersistenceHandleKey: EnvironmentKey {
    static let defaultValue: PersistenceHandle? = nil
}

extension EnvironmentValues {
    /// Process-wide SQLite persistence handle. `nil` in contexts that have not
    /// opened the database (tests, extensions, first-unlock race).
    public var persistenceHandle: PersistenceHandle? {
        get { self[PersistenceHandleKey.self] }
        set { self[PersistenceHandleKey.self] = newValue }
    }
}

extension PersistenceHandle {
    /// Insert a `snapshots` row immediately and install the block-finalize
    /// sink on the live terminal so each completed block writes a row.
    ///
    /// Lifetime contract: `terminal` (and the `PersistenceHandle` itself)
    /// must outlive this call. Callers must invoke `recordKill` exactly
    /// once before the terminal is freed.
    func attach(terminal: TerminalCore, snapshotID: UUID, hostID: UUID) {
        snapshotID.uuidString.withCString { sid in
            hostID.uuidString.withCString { hid in
                bt_term_attach_persistence(
                    terminal.unsafeHandle,
                    unsafeHandle,
                    sid, hid
                )
            }
        }
    }

    /// Open a snapshot for read-only replay. Returns a `TerminalCore` that
    /// has already replayed the stored block bytes; mount it in a
    /// `TerminalReplayHostView` with `isInputDisabled: true`.
    ///
    /// Returns `nil` if the snapshot is missing or the FFI returns null.
    func openReplay(snapshotID: UUID) -> TerminalCore? {
        var result: OpaquePointer?
        snapshotID.uuidString.withCString { cstr in
            result = bedterm_persistence_open_replay(unsafeHandle, cstr)
        }
        guard let ptr = result else { return nil }
        return TerminalCore(adoptedHandle: ptr)
    }

    /// Close the snapshot row: sets `kill_reason` and `killed_at`; also
    /// updates `last_cwd / last_command / last_exit_code` from the
    /// last-finalized block if available.
    func recordKill(
        snapshotID: UUID,
        reason: SessionSnapshot.KillReason,
        lastCwd: String?,
        lastCommand: String?,
        lastExitCode: Int32?
    ) {
        let reasonInt: Int32
        switch reason {
        case .userKilled: reasonInt = 0
        case .remoteLogout: reasonInt = 1
        case .networkDrop: reasonInt = 2
        case .appRelaunch: reasonInt = 3
        case .swapEvicted: reasonInt = 4
        }
        snapshotID.uuidString.withCString { sid in
            let cwdHandler: (UnsafePointer<CChar>?) -> Void = { cwdPtr in
                let cmdHandler: (UnsafePointer<CChar>?) -> Void = { cmdPtr in
                    bedterm_persistence_record_kill(
                        self.unsafeHandle,
                        sid,
                        reasonInt,
                        cwdPtr, cmdPtr,
                        lastExitCode ?? 0,
                        lastExitCode == nil ? 0 : 1
                    )
                }
                if let cmd = lastCommand {
                    cmd.withCString(cmdHandler)
                } else {
                    cmdHandler(nil)
                }
            }
            if let cwd = lastCwd {
                cwd.withCString(cwdHandler)
            } else {
                cwdHandler(nil)
            }
        }
    }
}
