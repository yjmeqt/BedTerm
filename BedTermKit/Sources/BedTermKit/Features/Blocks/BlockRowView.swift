import SwiftUI

/// One row in the Block list: a header (status dot, command, exit info,
/// duration) and an expanded drawer hosting `BlockMetalView` that renders
/// the block's row range from the shared terminal grid.
struct BlockRowView: View {
    let block: Block
    /// Live core for running blocks. `nil` for sealed blocks — those
    /// render exclusively from their `frozenSnapshot`.
    let core: TerminalCore?
    let isExpanded: Bool
    let onToggle: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            if isExpanded {
                drawer
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .background(Color("ShadcnCard", bundle: .module))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .animation(.smooth(duration: 0.18), value: isExpanded)
    }

    @ViewBuilder
    private var header: some View {
        Button(action: onToggle) {
            HStack(alignment: .center, spacing: 10) {
                statusDot
                VStack(alignment: .leading, spacing: 2) {
                    Text(displayCommand)
                        .font(.system(.subheadline, design: .monospaced))
                        .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if let subtitle = headerSubtitle {
                        Text(subtitle)
                            .font(.caption2)
                            .foregroundStyle(
                                Color("ShadcnMutedForeground", bundle: .module))
                    }
                }
                Spacer(minLength: 0)
                Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var drawer: some View {
        Divider()
            .background(Color("ShadcnBorder", bundle: .module))
        if let source = metalSource {
            BlockMetalView(source: source)
                .frame(height: metalViewHeight(for: source))
        } else {
            Text(placeholderBody)
                .font(.system(.footnote, design: .monospaced))
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
        }
    }

    // MARK: - Source selection

    private var metalSource: BlockMetalView.Source? {
        if let snap = block.frozenSnapshot {
            return .frozen(snap)
        }
        if block.isRunning, let core {
            return .live(core: core, startLine: block.startLine)
        }
        return nil
    }

    private func metalViewHeight(for source: BlockMetalView.Source) -> CGFloat {
        // Approximate: scale by row count. Real cell height comes from the
        // renderer atlas (see `bt_renderer_cell_pixel_size`); for v1 use a
        // sensible baseline that gives sealed and short running blocks the
        // space they need without over-claiming when the block is tall.
        let rows: Int = {
            switch source {
            case .frozen(let snap):
                return Int(snap.rows)
            case .live(let core, let start):
                let end = core.currentLine + 1
                return max(0, Int(end - start))
            }
        }()
        let rowHeight: CGFloat = 18
        let minHeight: CGFloat = 36
        let maxHeight: CGFloat = 360
        return min(max(CGFloat(rows) * rowHeight, minHeight), maxHeight)
    }

    // MARK: - Header derivation

    private var displayCommand: String {
        block.command.isEmpty
            ? String(localized: "(no command captured)")
            : block.command
    }

    private var headerSubtitle: String? {
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

    private var placeholderBody: String {
        block.isRunning
            ? String(localized: "(output streaming…)")
            : String(localized: "(no output captured)")
    }

    private func formatDuration(_ seconds: TimeInterval) -> String {
        if seconds < 1.0 {
            return String(format: "%.0fms", seconds * 1000)
        }
        if seconds < 60 {
            return String(format: "%.1fs", seconds)
        }
        let minutes = Int(seconds) / 60
        let secs = Int(seconds) % 60
        return "\(minutes)m \(secs)s"
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
}
