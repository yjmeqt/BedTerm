import SwiftUI

struct HostRow: View {
    let entry: SavedHost
    let inFlight: Bool
    let error: HostsViewModel.RowError?
    let isCurrentSession: Bool
    let onRowBodyTap: () -> Void
    let onConnect: () -> Void
    let onEdit: () -> Void
    let onRetry: () -> Void
    let onOpenSettings: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            Button(action: onRowBodyTap) {
                VStack(alignment: .leading, spacing: 8) {
                    primaryAndSubtitle
                    if let error, error.expanded {
                        expandedError(error)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(inFlight ? 0.6 : 1.0)
            actionStack
        }
    }

    private var actionStack: some View {
        HStack(spacing: 6) {
            authBadge
            editButton
            connectButton
        }
    }

    /// Always-visible edit affordance — for users who don't know about
    /// swipe-actions. The swipe and context-menu paths still work.
    private var editButton: some View {
        Button(action: onEdit) {
            Image(systemName: "pencil")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(8)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text("Edit"))
        .accessibilityIdentifier("hosts.row.edit")
    }

    /// Primary connect affordance — replaces tap-the-whole-row from earlier
    /// drafts. The list is now display + storage; this button is the only way
    /// the row starts a connection.
    private var connectButton: some View {
        Button(action: onConnect) {
            if inFlight {
                ProgressView()
                    .controlSize(.small)
                    .frame(width: 24, height: 24)
            } else {
                Image(systemName: "play.fill")
                    .font(.callout)
                    .frame(width: 24, height: 24)
            }
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.small)
        .tint(.blue)
        .disabled(inFlight)
        .accessibilityLabel(Text("Connect"))
        .accessibilityIdentifier("hosts.row.connect")
    }

    private var primaryAndSubtitle: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(primaryLabel)
                .font(.callout)
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            if let error, !error.expanded {
                collapsedError(error)
            } else if let subtitle {
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospaced()
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
    }

    private func collapsedError(_ err: HostsViewModel.RowError) -> some View {
        HStack(spacing: 6) {
            Text("error")
                .font(.caption2.weight(.semibold))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(Color.red, in: RoundedRectangle(cornerRadius: 4))
                .foregroundStyle(.white)
            Text(err.message)
                .font(.caption)
                .foregroundStyle(.red)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }

    private func expandedError(_ err: HostsViewModel.RowError) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 6) {
                Text("error")
                    .font(.caption2.weight(.semibold))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Color.red, in: RoundedRectangle(cornerRadius: 4))
                    .foregroundStyle(.white)
                Text(err.message)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 8) {
                if err.permissionDenied {
                    Button(String(localized: "Open Settings"), action: onOpenSettings)
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                }
                Button(String(localized: "Retry"), action: onRetry)
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
            }
        }
    }

    private var primaryLabel: String {
        if !entry.label.isEmpty { return entry.label }
        return "\(entry.credential.username)@\(formattedHost)"
    }

    /// The "user@host[:port]" subtitle, suppressed entirely when it would
    /// duplicate the primary line (empty label + port 22).
    private var subtitle: String? {
        if entry.label.isEmpty && entry.credential.port == 22 { return nil }
        var subtitle = "\(entry.credential.username)@\(formattedHost)"
        if entry.credential.port != 22 {
            subtitle += ":\(entry.credential.port)"
        }
        return subtitle
    }

    /// Wraps IPv6 literals in brackets so the `:port` separator is unambiguous.
    /// Preserves IPv6 zone identifiers ("%en0") verbatim.
    private var formattedHost: String {
        let host = entry.credential.host
        if host.contains(":") && !host.hasPrefix("[") {
            return "[\(host)]"
        }
        return host
    }

    private var authBadge: some View {
        let isKey: Bool
        switch entry.credential.auth {
        case .privateKey: isKey = true
        case .password: isKey = false
        }
        let symbol = isKey ? "key.fill" : "lock.fill"
        let bg = isKey ? Color(.systemGray5) : Color.orange
        let fg = isKey ? Color.primary : Color.white
        return Image(systemName: symbol)
            .font(.caption.weight(.semibold))
            .frame(width: 14, height: 14)
            .padding(.horizontal, 7)
            .padding(.vertical, 4)
            .background(bg, in: RoundedRectangle(cornerRadius: 6))
            .foregroundStyle(fg)
            .accessibilityLabel(
                Text(
                    isKey
                        ? String(localized: "Authenticates with a private key")
                        : String(localized: "Authenticates with a password"))
            )
    }
}
