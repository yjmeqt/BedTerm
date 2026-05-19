import SwiftUI

/// Flat header strip shown above a block's Metal-painted body. No
/// chevron, no tap-to-toggle — Phase B blocks are always expanded
/// (matches Warp's default). Hosted via UIHostingController inside
/// BlockListContainerView's UIScrollView content view.
struct BlockHeader: View {
    let block: Block

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            statusDot
            VStack(alignment: .leading, spacing: 2) {
                Text(displayCommand)
                    .font(.system(.subheadline, design: .monospaced))
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                    .lineLimit(1)
                    .truncationMode(.tail)
                if let subtitle = subtitle {
                    Text(subtitle)
                        .font(.caption2)
                        .foregroundStyle(
                            Color("ShadcnMutedForeground", bundle: .module))
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
    }

    @ViewBuilder
    private var statusDot: some View {
        Circle()
            .fill(statusColor)
            .frame(width: 8, height: 8)
            .padding(2)
    }

    private var statusColor: Color {
        if block.isRunning {
            return Color("ShadcnMutedForeground", bundle: .module)
        }
        switch block.exitCode {
        case .some(0):
            return Color("ShadcnPrimary", bundle: .module)
        case .some:
            return Color("ShadcnDestructive", bundle: .module)
        case .none:
            return Color("ShadcnMutedForeground", bundle: .module)
        }
    }

    private var displayCommand: String {
        block.command.isEmpty
            ? String(localized: "(no command captured)")
            : block.command
    }

    private var subtitle: String? {
        var parts: [String] = []
        if let exit = block.exitCode {
            parts.append("exit \(exit)")
        } else if block.isRunning {
            parts.append(String(localized: "running…"))
        }
        if let dur = block.duration {
            parts.append(formatDuration(dur))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func formatDuration(_ seconds: TimeInterval) -> String {
        if seconds < 1.0 { return String(format: "%.0fms", seconds * 1000) }
        if seconds < 60 { return String(format: "%.1fs", seconds) }
        let minutes = Int(seconds) / 60
        let secs = Int(seconds) % 60
        return "\(minutes)m \(secs)s"
    }
}
