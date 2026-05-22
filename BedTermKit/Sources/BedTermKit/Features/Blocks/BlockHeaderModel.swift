import Foundation
import UIKit

/// Value-type carrier for the data the Rust renderer needs to paint one
/// block header band. Replaces the `BlockHeader` SwiftUI view + its
/// `agentTints` table (now centralised here). Holds no UIKit handles
/// so it's safe to build off the main actor for tests.
struct BlockHeaderModel {
    let blockID: UInt64
    let command: String
    let subtitle: String?
    let agent: CLIAgent?

    /// 0xRRGGBBAA brand tint for each agent's badge background. Same
    /// values BlockHeader.swift used to define inline.
    private static let agentTints: [CLIAgent: UInt32] = [
        .claude: 0xD66E_4DFF,
        .codex: 0x6BB6_B1FF,
        .gemini: 0x4D8C_F2FF,
        .amp: 0xFFC8_22FF,
        .droid: 0x9CC4_33FF,
        .openCode: 0x7373_D9FF,
        .copilot: 0x2233_52FF,
        .pi: 0xF0EB_D6FF,
        .auggie: 0x3333_33FF,
        .cursorCli: 0x2121_21FF,
        .goose: 0xA672_2EFF,
        .hermes: 0x8252_BCFF,
        .vibe: 0xF27E_2EFF
    ]

    /// Default neutral grey for unmapped agents — preserves the
    /// "row stays visually consistent" rule from the old SwiftUI view.
    static func tint(for agent: CLIAgent?) -> UInt32 {
        guard let a = agent else { return 0 }
        return agentTints[a] ?? 0x8080_80FF
    }

    /// Maps to Rust's `IconSlot` via `bedterm_core/icon_atlas.rs` —
    /// raw value of the enum (matches CLIAgent.rawValue 1..13), or 0
    /// when no agent is identified. Anything > 0 that isn't claude (1)
    /// or codex (3) falls back to the generic sparkle on the Rust side.
    static func agentID(_ agent: CLIAgent?) -> UInt8 {
        guard let a = agent else { return 0 }
        return a.rawValue
    }

    static func displayCommand(for block: Block) -> String {
        block.command.isEmpty
            ? String(localized: "(no command captured)")
            : block.command
    }

    static func subtitle(for block: Block) -> String? {
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

    private static func formatDuration(_ seconds: TimeInterval) -> String {
        if seconds < 1.0 { return String(format: "%.0fms", seconds * 1000) }
        if seconds < 60 { return String(format: "%.1fs", seconds) }
        let m = Int(seconds) / 60
        let s = Int(seconds) % 60
        return "\(m)m \(s)s"
    }
}

extension UIColor {
    /// Pack a `UIColor` as `0xRRGGBBAA` for the Rust renderer's RGBA32
    /// fields. Resolves the colour against the caller's trait
    /// collection first via `compatibleWith:`, so dark-mode tokens
    /// land with the right component values.
    func asRGBA32() -> UInt32 {
        var r: CGFloat = 0
        var g: CGFloat = 0
        var b: CGFloat = 0
        var a: CGFloat = 0
        guard getRed(&r, green: &g, blue: &b, alpha: &a) else { return 0 }
        let red = UInt32(max(0, min(255, Int((r * 255).rounded()))))
        let green = UInt32(max(0, min(255, Int((g * 255).rounded()))))
        let blue = UInt32(max(0, min(255, Int((b * 255).rounded()))))
        let alpha = UInt32(max(0, min(255, Int((a * 255).rounded()))))
        return (red << 24) | (green << 16) | (blue << 8) | alpha
    }
}
