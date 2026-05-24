import SwiftUI

/// Read-only view of a killed session: header metadata, a Metal replay of
/// the terminal output, and two affordances to keep working — Resume here
/// (re-launch with `cd <cwd>` queued) or New shell (fresh `$HOME`).
public struct KilledSessionDetailScreen: View {
    @Binding var path: NavigationPath
    @Environment(PersistedSessionSnapshotStore.self) private var store
    @Environment(\.persistenceHandle) private var persistence: PersistenceHandle?

    @State private var replayCore: TerminalCore?
    @State private var loadFailed = false

    let snapshotID: UUID
    let host: SavedHost
    let onResume: (_ initialInput: String?) -> Void

    public init(
        path: Binding<NavigationPath>,
        snapshotID: UUID,
        host: SavedHost,
        onResume: @escaping (_ initialInput: String?) -> Void
    ) {
        self._path = path
        self.snapshotID = snapshotID
        self.host = host
        self.onResume = onResume
    }

    public var body: some View {
        Group {
            if let snapshot = store.snapshot(id: snapshotID) {
                content(snapshot)
            } else {
                Text("Session no longer available.")
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            }
        }
        .background(Color("ShadcnBackground", bundle: .module).ignoresSafeArea())
        .navigationTitle(Text("Killed session"))
        .navigationBarTitleDisplayMode(.inline)
        .task {
            guard let persistence else { loadFailed = true; return }
            if let core = persistence.openReplay(snapshotID: snapshotID) {
                replayCore = core
            } else {
                loadFailed = true
            }
        }
    }

    @ViewBuilder
    private func content(_ snapshot: SessionSnapshot) -> some View {
        VStack(spacing: 0) {
            header(snapshot)
            Divider()
                .background(Color("ShadcnBorder", bundle: .module))
            replayBody()
            actionBar(snapshot)
        }
    }

    private func header(_ snapshot: SessionSnapshot) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(verbatim: hostName)
                .font(.headline)
                .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
            Text(String(localized: "Killed \(relativeTimestamp(snapshot)) — \(snapshot.killReason.localizedReason)"))
                .font(.footnote)
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            if let cwd = abbreviatedCwd(snapshot) {
                HStack(spacing: 4) {
                    Image(systemName: "folder")
                        .font(.caption2)
                    Text(verbatim: cwd)
                        .font(.footnote.monospaced())
                }
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
    }

    @ViewBuilder
    private func replayBody() -> some View {
        if let core = replayCore {
            TerminalReplayHostView(replayCore: core)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .ignoresSafeArea(edges: .horizontal)
        } else if loadFailed {
            Text("Session output no longer available.")
                .font(.footnote)
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func actionBar(_ snapshot: SessionSnapshot) -> some View {
        VStack(spacing: 8) {
            Divider()
                .background(Color("ShadcnBorder", bundle: .module))
            HStack(spacing: 12) {
                if let cwd = snapshot.lastCwd, !cwd.isEmpty {
                    Button {
                        onResume(Self.buildResumeCommand(cwd: cwd))
                    } label: {
                        actionLabel(text: String(localized: "Resume here"), isPrimary: true)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("sessions.detail.resume")
                }
                Button {
                    onResume(nil)
                } label: {
                    actionLabel(text: String(localized: "New shell"), isPrimary: false)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("sessions.detail.newShell")
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 12)
            .padding(.top, 8)
        }
    }

    private func actionLabel(text: String, isPrimary: Bool) -> some View {
        Text(text)
            .font(.footnote.weight(.medium))
            .frame(maxWidth: .infinity)
            .frame(height: 40)
            .foregroundStyle(
                isPrimary
                    ? Color("ShadcnPrimaryForeground", bundle: .module)
                    : Color("ShadcnPrimary", bundle: .module)
            )
            .background(
                isPrimary
                    ? Color("ShadcnPrimary", bundle: .module)
                    : Color("ShadcnCard", bundle: .module)
            )
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
            )
    }

    private var hostName: String {
        if !host.label.isEmpty { return host.label }
        return "\(host.credential.username)@\(host.credential.host)"
    }

    private func abbreviatedCwd(_ snapshot: SessionSnapshot) -> String? {
        guard let cwd = snapshot.lastCwd, !cwd.isEmpty else { return nil }
        return SessionSnapshotPathAbbreviator.abbreviate(cwd)
    }

    private func relativeTimestamp(_ snapshot: SessionSnapshot) -> String {
        let fmt = RelativeDateTimeFormatter()
        fmt.unitsStyle = .full
        return fmt.localizedString(for: snapshot.killedAt, relativeTo: .now)
    }

    /// PRD R5.cwd_resume_quoting: single-quote the path, escape embedded
    /// single quotes as `'\''`, terminate with a newline so the shell
    /// executes immediately.
    static func buildResumeCommand(cwd: String) -> String {
        let escaped = cwd.replacingOccurrences(of: "'", with: "'\\''")
        return "cd '\(escaped)'\n"
    }
}

/// Visible for tests + cross-file reuse — kept file-private otherwise.
enum SessionSnapshotPathAbbreviator {
    static func abbreviate(_ path: String) -> String {
        let patterns = ["/home/", "/Users/"]
        for prefix in patterns where path.hasPrefix(prefix) {
            let rest = path.dropFirst(prefix.count)
            if let slash = rest.firstIndex(of: "/") {
                return "~" + rest[slash...]
            }
            return "~"
        }
        if path == "/root" || path.hasPrefix("/root/") {
            return "~" + path.dropFirst("/root".count)
        }
        return path
    }
}
