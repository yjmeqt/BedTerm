import SwiftUI

/// Flat header strip shown above a block's Metal-painted body. No
/// chevron, no tap-to-toggle — Phase B blocks are always expanded
/// (matches Warp's default). Hosted via UIHostingController inside
/// BlockListContainerViewController's UIScrollView content view.
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
        if block.isRunning {
            // Pulsing dot signals streaming output. Animation lives on a
            // dedicated subview so its @State survives BlockHeader
            // rebuilds (the parent re-renders on every CADisplayLink
            // tick; identity is preserved at the same tree position).
            PulsingStatusDot(color: Color("ShadcnMutedForeground", bundle: .module))
        } else {
            Circle()
                .fill(sealedStatusColor)
                .frame(width: 8, height: 8)
                .padding(2)
        }
    }

    private var sealedStatusColor: Color {
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
            parts.append(String(localized: "exit \(exit)"))
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

/// Self-animating dot that breathes between full and dim. Used as the
/// running-block status indicator. SwiftUI's repeatForever animation
/// runs on Core Animation, independent of the parent's CADisplayLink
/// — the dot doesn't draw additional frames just because BlockHeader's
/// rootView is reassigned each tick.
private struct PulsingStatusDot: View {
    let color: Color
    @State private var pulse = false

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 8, height: 8)
            .opacity(pulse ? 0.35 : 1.0)
            .animation(
                .easeInOut(duration: 0.9).repeatForever(autoreverses: true),
                value: pulse
            )
            .onAppear { pulse = true }
            .padding(2)
    }
}
