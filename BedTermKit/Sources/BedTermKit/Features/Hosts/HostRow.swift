import SwiftUI

struct HostRow: View {
    let entry: SavedHost
    let inFlight: Bool
    let isCurrentSession: Bool
    let onTapBody: () -> Void
    let onConnect: () -> Void

    var body: some View {
        Button(action: onTapBody) {
            HStack(alignment: .center, spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(primaryLabel)
                        .font(.callout.weight(.medium))
                        .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if let subtitle {
                        Text(subtitle)
                            .font(.footnote)
                            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                authBadge
                connectButton
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(Color("ShadcnCard", bundle: .module))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .opacity(inFlight ? 0.7 : 1.0)
    }

    private var connectButton: some View {
        Button(action: onConnect) {
            HStack(spacing: 4) {
                if inFlight {
                    ProgressView()
                        .controlSize(.mini)
                        .tint(Color("ShadcnPrimaryForeground", bundle: .module))
                } else {
                    Image(systemName: "play.fill").font(.caption.weight(.semibold))
                    Text("Connect").font(.footnote.weight(.medium))
                }
            }
            .padding(.horizontal, 10)
            .frame(height: 28)
            .foregroundStyle(Color("ShadcnPrimaryForeground", bundle: .module))
            .background(Color("ShadcnPrimary", bundle: .module))
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .disabled(inFlight)
        .accessibilityLabel(Text("Connect"))
        .accessibilityIdentifier("hosts.row.connect")
    }

    private var authBadge: some View {
        let isKey: Bool
        switch entry.credential.auth {
        case .privateKey: isKey = true
        case .password: isKey = false
        }
        let symbol = isKey ? "key.fill" : "lock.fill"
        return Image(systemName: symbol)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            .frame(width: 24, height: 24)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
            )
            .accessibilityLabel(
                Text(
                    isKey
                        ? String(localized: "Authenticates with a private key")
                        : String(localized: "Authenticates with a password"))
            )
    }

    private var primaryLabel: String {
        if !entry.label.isEmpty { return entry.label }
        return "\(entry.credential.username)@\(formattedHost)"
    }

    private var subtitle: String? {
        if entry.label.isEmpty && entry.credential.port == 22 { return nil }
        var subtitle = "\(entry.credential.username)@\(formattedHost)"
        if entry.credential.port != 22 {
            subtitle += ":\(entry.credential.port)"
        }
        return subtitle
    }

    private var formattedHost: String {
        let host = entry.credential.host
        if host.contains(":") && !host.hasPrefix("[") {
            return "[\(host)]"
        }
        return host
    }
}
