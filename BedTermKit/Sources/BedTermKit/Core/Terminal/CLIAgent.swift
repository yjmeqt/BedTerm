import Foundation

/// CLI agent identified at the block's `Preexec` boundary. Mirrors
/// the Rust-side `CliAgent` enum + `BT_CLI_AGENT_*` FFI tag. Used to
/// paint a brand badge next to the block header; does NOT influence
/// layout (alt-screen vs inline-CLI vs default block is decided by
/// the byte stream, not by which agent is running).
public enum CLIAgent: UInt8, Sendable, Equatable {
    case claude = 1
    case gemini = 2
    case codex = 3
    case amp = 4
    case droid = 5
    case openCode = 6
    case copilot = 7
    case pi = 8
    case auggie = 9
    case cursorCli = 10
    case goose = 11
    case hermes = 12
    case vibe = 13

    /// Decode the FFI tag. Returns `nil` for `BT_CLI_AGENT_NONE` (`0`)
    /// or unknown tags (forward-compat with future Rust additions).
    public init?(ffiTag: UInt8) {
        guard ffiTag != 0 else { return nil }
        self.init(rawValue: ffiTag)
    }

    /// Human-readable product name. Matches Warp's labelling.
    public var displayName: String {
        switch self {
        case .claude: return "Claude Code"
        case .gemini: return "Gemini CLI"
        case .codex: return "Codex"
        case .amp: return "Amp"
        case .droid: return "Droid"
        case .openCode: return "OpenCode"
        case .copilot: return "GitHub Copilot"
        case .pi: return "Pi"
        case .auggie: return "Auggie"
        case .cursorCli: return "Cursor"
        case .goose: return "Goose"
        case .hermes: return "Hermes"
        case .vibe: return "Mistral Vibe"
        }
    }

    /// Asset-catalog name for the brand icon. Only populated for
    /// agents whose SVG we've bundled. Others return `nil`, letting
    /// the call site fall back to a generic glyph.
    public var iconName: String? {
        switch self {
        case .claude: return "ClaudeLogo"
        case .codex: return "OpenAILogo"
        default: return nil
        }
    }
}
