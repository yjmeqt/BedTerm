import SwiftUI

/// The Warp-style left-pane equivalent: a vertical list of killed
/// sessions under a single host (PRD R2). v1 lists killed sessions
/// only — running sessions don't survive the navigation pop yet (P3).
public struct SessionsPanelScreen: View {
    @Binding var path: NavigationPath
    @Environment(PersistedSessionSnapshotStore.self) private var store

    let host: SavedHost
    let onStartNew: () -> Void

    public init(
        path: Binding<NavigationPath>,
        host: SavedHost,
        onStartNew: @escaping () -> Void
    ) {
        self._path = path
        self.host = host
        self.onStartNew = onStartNew
    }

    public var body: some View {
        let snapshots = store.snapshots(forHost: host.id)
        Group {
            if snapshots.isEmpty {
                emptyState
            } else {
                listContent(snapshots)
            }
        }
        .background(Color("ShadcnBackground", bundle: .module).ignoresSafeArea())
        .task { store.reload(forHost: host.id) }
        .navigationTitle(Text(verbatim: panelTitle))
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    onStartNew()
                } label: {
                    Image(systemName: "plus")
                }
                .accessibilityLabel(Text("New session"))
                .accessibilityIdentifier("sessions.panel.new")
            }
        }
    }

    @ViewBuilder
    private func listContent(_ snapshots: [SessionSnapshot]) -> some View {
        ScrollView {
            LazyVStack(spacing: 12) {
                ForEach(snapshots) { snapshot in
                    Button {
                        path.append(AppRoute.killedSessionDetail(snapshot.id))
                    } label: {
                        SessionSnapshotRow(snapshot: snapshot)
                    }
                    .buttonStyle(.plain)
                    .contextMenu {
                        Button(
                            String(localized: "Discard"),
                            systemImage: "trash",
                            role: .destructive
                        ) {
                            store.discard(id: snapshot.id)
                        }
                    }
                }
            }
            .padding(16)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 16) {
            Image(systemName: "rectangle.stack")
                .font(.title)
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                .frame(width: 56, height: 56)
                .background(
                    RoundedRectangle(cornerRadius: 12)
                        .fill(Color("ShadcnCard", bundle: .module))
                        .overlay(
                            RoundedRectangle(cornerRadius: 12)
                                .stroke(
                                    Color("ShadcnBorder", bundle: .module), lineWidth: 1)
                        )
                )
            VStack(spacing: 4) {
                Text("No past sessions")
                    .font(.headline)
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                Text("Start a session — when it ends, it'll show up here.")
                    .font(.subheadline)
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                    .multilineTextAlignment(.center)
            }
            Button {
                onStartNew()
            } label: {
                Text("New session")
                    .font(.footnote.weight(.medium))
                    .padding(.horizontal, 16)
                    .frame(height: 36)
                    .foregroundStyle(Color("ShadcnPrimaryForeground", bundle: .module))
                    .background(Color("ShadcnPrimary", bundle: .module))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("sessions.panel.emptyState.new")
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var panelTitle: String {
        if !host.label.isEmpty { return host.label }
        return "\(host.credential.username)@\(host.credential.host)"
    }
}

private struct SessionSnapshotRow: View {
    let snapshot: SessionSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Inline summary of the snapshot's last block (BlockHeader
            // the SwiftUI view was retired when the live block list went
            // pure-Metal; we only need a static thumbnail here).
            if let block = lastBlock {
                HStack(spacing: 8) {
                    Text(verbatim: block.command.isEmpty ? "—" : block.command)
                        .font(.system(.subheadline, design: .monospaced))
                        .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if let exit = block.exitCode {
                        Text(verbatim: "\(exit)")
                            .font(.caption2.weight(.semibold).monospaced())
                            .foregroundStyle(Color("ShadcnPrimaryForeground", bundle: .module))
                            .padding(.horizontal, 6)
                            .frame(height: 18)
                            .background(
                                exit == 0
                                    ? Color("ShadcnMutedForeground", bundle: .module)
                                    : Color("ShadcnDestructive", bundle: .module)
                            )
                            .clipShape(RoundedRectangle(cornerRadius: 4))
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
            } else {
                // No blocks captured: fall back to a plain label.
                Text(verbatim: "—")
                    .font(.system(.subheadline, design: .monospaced))
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
            }
            Divider()
                .padding(.horizontal, 12)
            HStack {
                Text(verbatim: relativeTimestamp)
                    .font(.caption2)
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
        }
        .contentShape(Rectangle())
        .background(Color("ShadcnCard", bundle: .module))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(verbatim: accessibilityLabel))
    }

    private var lastBlock: Block? {
        snapshot.blocks.last(where: { !$0.isRunning })
            ?? snapshot.blocks.last
    }

    private var relativeTimestamp: String {
        let fmt = RelativeDateTimeFormatter()
        fmt.unitsStyle = .short
        return fmt.localizedString(for: snapshot.killedAt, relativeTo: .now)
    }

    private var accessibilityLabel: String {
        let cmd = lastBlock.flatMap { $0.command.isEmpty ? nil : $0.command } ?? "—"
        let when = relativeTimestamp
        let reason = snapshot.killReason.localizedReason
        return "\(cmd), killed \(when), \(reason)"
    }
}
