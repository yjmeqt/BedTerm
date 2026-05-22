import SwiftUI

/// Read-only view of a killed session: header metadata, the list of
/// commands that ran, and two affordances to keep working — Resume here
/// (re-launch with `cd <cwd>` queued) or New shell (fresh `$HOME`).
public struct KilledSessionDetailScreen: View {
    @Binding var path: NavigationPath
    @Environment(PersistedSessionSnapshotStore.self) private var store

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
    }

    @ViewBuilder
    private func content(_ snapshot: SessionSnapshot) -> some View {
        VStack(spacing: 0) {
            header(snapshot)
            Divider()
                .background(Color("ShadcnBorder", bundle: .module))
            blockList(snapshot)
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

    private func blockList(_ snapshot: SessionSnapshot) -> some View {
        ScrollView {
            LazyVStack(spacing: 8) {
                if snapshot.blocks.isEmpty {
                    Text("No commands were captured before this session ended.")
                        .font(.footnote)
                        .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        .padding(.top, 24)
                } else {
                    ForEach(snapshot.blocks) { block in
                        SessionBlockRow(block: block)
                    }
                }
            }
            .padding(16)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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

private struct SessionBlockRow: View {
    let block: Block

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                exitChip
                Text(verbatim: commandLabel)
                    .font(.callout.monospaced())
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                    .lineLimit(2)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            if let metadata {
                Text(verbatim: metadata)
                    .font(.caption2.monospaced())
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color("ShadcnCard", bundle: .module))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var commandLabel: String {
        block.command.isEmpty ? "—" : block.command
    }

    private var exitChip: some View {
        let exit = block.exitCode
        let label: String
        let bg: Color
        if let exit {
            label = "\(exit)"
            bg =
                exit == 0
                ? Color("ShadcnMutedForeground", bundle: .module)
                : Color("ShadcnDestructive", bundle: .module)
        } else {
            label = "—"
            bg = Color("ShadcnMutedForeground", bundle: .module)
        }
        return Text(verbatim: label)
            .font(.caption2.weight(.semibold).monospaced())
            .foregroundStyle(Color("ShadcnPrimaryForeground", bundle: .module))
            .padding(.horizontal, 8)
            .frame(height: 18)
            .background(bg)
            .clipShape(RoundedRectangle(cornerRadius: 4))
    }

    private var metadata: String? {
        var parts: [String] = []
        if let cwd = block.workingDirectory, !cwd.isEmpty {
            parts.append(SessionSnapshotPathAbbreviator.abbreviate(cwd))
        }
        if let branch = block.gitBranch, !branch.isEmpty {
            parts.append("(\(branch))")
        }
        if let dur = block.duration {
            parts.append(formatDuration(dur))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func formatDuration(_ seconds: TimeInterval) -> String {
        if seconds < 1 {
            return String(format: "%dms", Int(seconds * 1000))
        } else if seconds < 60 {
            return String(format: "%.1fs", seconds)
        }
        let minutes = Int(seconds) / 60
        let remSeconds = Int(seconds) % 60
        return "\(minutes)m \(remSeconds)s"
    }
}
