import BedTermCoreC
import Foundation

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
