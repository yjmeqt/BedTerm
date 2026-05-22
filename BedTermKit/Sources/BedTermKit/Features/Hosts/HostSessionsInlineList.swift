import SwiftUI

/// Inline list of a host's sessions, rendered directly beneath the
/// host row on the Hosts list when the session count is small (PRD
/// background-sessions R2.inline_sessions_on_host_row).
///
/// Visibility rules:
///   - 0 sessions  → render nothing (caller skips this view entirely).
///   - 1…5 sessions → render every session as a compact row.
///   - >5 sessions  → render only a "View all N sessions" affordance
///     that pushes the full `SessionsPanelScreen` destination.
///
/// The running session (if any) appears first; killed snapshots follow
/// in newest-first order (matching `SessionSnapshotStore` insertion).
struct HostSessionsInlineList: View {
    static let inlineThreshold = 5

    let host: SavedHost
    let runningSessionID: UUID?
    let snapshots: [SessionSnapshot]
    let onTapRunning: () -> Void
    let onTapKilled: (SessionSnapshot) -> Void
    let onViewAll: () -> Void

    var body: some View {
        let totalCount = snapshots.count + (hasRunning ? 1 : 0)
        if totalCount == 0 {
            EmptyView()
        } else if totalCount > Self.inlineThreshold {
            viewAllRow(count: totalCount)
                .padding(.leading, 16)
        } else {
            VStack(spacing: 6) {
                if hasRunning {
                    runningRow
                }
                ForEach(snapshots) { snapshot in
                    Button {
                        onTapKilled(snapshot)
                    } label: {
                        InlineSessionRow(
                            statusColor: snapshotStatusColor(snapshot),
                            statusFilled: (snapshot.lastExitCode ?? 0) != 0,
                            primaryText: snapshot.lastCommand.flatMap {
                                $0.isEmpty ? nil : $0
                            } ?? "—",
                            secondaryText: relativeTimestamp(snapshot.killedAt)
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.leading, 16)
        }
    }

    private var hasRunning: Bool {
        runningSessionID == host.id
    }

    private var runningRow: some View {
        Button(action: onTapRunning) {
            InlineSessionRow(
                statusColor: Color("ShadcnPrimary", bundle: .module),
                statusFilled: true,
                primaryText: String(localized: "Active session"),
                secondaryText: String(localized: "Tap to return")
            )
        }
        .buttonStyle(.plain)
    }

    private func viewAllRow(count: Int) -> some View {
        Button(action: onViewAll) {
            HStack(spacing: 8) {
                Image(systemName: "rectangle.stack")
                    .font(.caption.weight(.medium))
                Text("View all \(count) sessions")
                    .font(.footnote.weight(.medium))
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.caption2.weight(.semibold))
            }
            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color("ShadcnCard", bundle: .module))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("hosts.row.viewAllSessions")
    }

    private func snapshotStatusColor(_ snapshot: SessionSnapshot) -> Color {
        let isError = (snapshot.lastExitCode ?? 0) != 0
        return isError
            ? Color("ShadcnDestructive", bundle: .module)
            : Color("ShadcnMutedForeground", bundle: .module)
    }

    private func relativeTimestamp(_ date: Date) -> String {
        let fmt = RelativeDateTimeFormatter()
        fmt.unitsStyle = .short
        return fmt.localizedString(for: date, relativeTo: .now)
    }
}

private struct InlineSessionRow: View {
    let statusColor: Color
    let statusFilled: Bool
    let primaryText: String
    let secondaryText: String?

    var body: some View {
        HStack(spacing: 10) {
            Circle()
                .strokeBorder(statusColor, lineWidth: 1.5)
                .background(Circle().fill(statusFilled ? statusColor : Color.clear))
                .frame(width: 8, height: 8)
            Text(verbatim: primaryText)
                .font(.footnote.weight(.medium))
                .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                .lineLimit(1)
                .truncationMode(.middle)
            if let secondaryText {
                Text(verbatim: secondaryText)
                    .font(.caption2)
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Image(systemName: "chevron.right")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color("ShadcnCard", bundle: .module))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }
}
