import SwiftUI

/// Flat header strip shown above a block's Metal-painted body. No
/// chevron, no tap-to-toggle — Phase B blocks are always expanded
/// (matches Warp's default). Hosted via UIHostingController inside
/// BlockListContainerViewController's UIScrollView content view.
struct BlockHeader: View {
    let block: Block

    var body: some View {
        // The container's per-block left accent bar carries block status;
        // the header itself shows command + metadata only, with an
        // optional CLI-agent brand badge to the left when the command
        // was identified as a known agent (Warp-style).
        HStack(alignment: .center, spacing: 8) {
            agentBadge
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
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
    }

    /// Brand glyph for the block's identified CLI agent — Anthropic
    /// spiral for Claude, OpenAI flower for Codex, etc. `nil` for
    /// commands the detector didn't match. Falls back to a generic
    /// sparkle for known-but-unbundled agents so the row stays
    /// visually consistent.
    @ViewBuilder
    private var agentBadge: some View {
        if let agent = block.cliAgent {
            ZStack {
                if let iconName = agent.iconName {
                    Image(iconName, bundle: .module)
                        .renderingMode(.template)
                        .resizable()
                        .scaledToFit()
                        .frame(width: 16, height: 16)
                } else {
                    Image(systemName: "sparkles")
                        .font(.system(size: 12, weight: .medium))
                        .frame(width: 16, height: 16)
                }
            }
            .foregroundStyle(.white)
            .frame(width: 24, height: 24)
            .background(
                Circle().fill(agentTint(for: agent))
            )
            .accessibilityLabel(agent.displayName)
        }
    }

    /// Brand-color tints for the badge background. Values are
    /// hand-picked to match Warp's `CLI_AGENT` color constants and
    /// stored as `0xRRGGBB` ints so the table stays compact (kept in
    /// a static dictionary so the lookup function stays under
    /// SwiftLint's cyclomatic-complexity cap).
    private static let agentTints: [CLIAgent: UInt32] = [
        .claude: 0xD66E_4D,    // CLAUDE_ORANGE
        .codex: 0x6BB6_B1,     // OpenAI teal
        .gemini: 0x4D8C_F2,    // Google blue
        .amp: 0xFFC8_22,
        .droid: 0x9CC4_33,
        .openCode: 0x7373_D9,
        .copilot: 0x2233_52,
        .pi: 0xF0EB_D6,
        .auggie: 0x3333_33,
        .cursorCli: 0x2121_21,
        .goose: 0xA672_2E,
        .hermes: 0x8252_BC,
        .vibe: 0xF27E_2E
    ]

    private func agentTint(for agent: CLIAgent) -> Color {
        let hex = Self.agentTints[agent] ?? 0x8080_80
        return Color(
            red: Double((hex >> 16) & 0xFF) / 255.0,
            green: Double((hex >> 8) & 0xFF) / 255.0,
            blue: Double(hex & 0xFF) / 255.0
        )
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
