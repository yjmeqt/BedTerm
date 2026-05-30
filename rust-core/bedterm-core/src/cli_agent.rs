//! CLI-agent identification.
//!
//! Mirrors Warp's `app/src/terminal/cli_agent.rs::CLIAgent::detect` —
//! a pure string-prefix match against the first non-env-var token of
//! the command line. No PTY byte sniffing, no process tracing; the
//! tagged `Preexec` JSON's `"command"` field is the only input.
//!
//! Identity is *only* for UI branding (icon, accent color, footer
//! chip). It does **not** drive layout choices — alt-screen vs
//! inline block vs default block stays decided by the terminal byte
//! stream (alacritty `TermMode::ALT_SCREEN`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliAgent {
    Claude,
    Gemini,
    Codex,
    Amp,
    Droid,
    OpenCode,
    Copilot,
    Pi,
    Auggie,
    /// Cursor CLI invokes itself as `agent`.
    CursorCli,
    Goose,
    Hermes,
    Vibe,
}

impl CliAgent {
    /// Command-line prefix that identifies this agent. Matches the
    /// resolved first word of the command after env-var stripping.
    pub fn command_prefix(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Droid => "droid",
            Self::OpenCode => "opencode",
            Self::Copilot => "copilot",
            Self::Pi => "pi",
            Self::Auggie => "auggie",
            Self::CursorCli => "agent",
            Self::Goose => "goose",
            Self::Hermes => "hermes",
            Self::Vibe => "vibe",
        }
    }

    /// Human-readable product name. Matches Warp's labelling and the
    /// Swift `CLIAgent.displayName` mapping.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Gemini => "Gemini CLI",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Droid => "Droid",
            Self::OpenCode => "OpenCode",
            Self::Copilot => "GitHub Copilot",
            Self::Pi => "Pi",
            Self::Auggie => "Auggie",
            Self::CursorCli => "Cursor",
            Self::Goose => "Goose",
            Self::Hermes => "Hermes",
            Self::Vibe => "Mistral Vibe",
        }
    }

    /// Asset-catalog name for the brand icon. Only populated for
    /// agents whose SVG we've bundled. Returns `None` for agents
    /// without a dedicated icon, letting the call site fall back to
    /// a generic glyph. Mirrors the Swift `CLIAgent.iconName` mapping.
    pub fn icon_name(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("ClaudeLogo"),
            Self::Codex => Some("OpenAILogo"),
            _ => None,
        }
    }

    /// Stable u8 tag for FFI. Matches `BT_CLI_AGENT_*` constants. New
    /// values append; never renumber.
    pub fn ffi_tag(self) -> u8 {
        match self {
            Self::Claude => 1,
            Self::Gemini => 2,
            Self::Codex => 3,
            Self::Amp => 4,
            Self::Droid => 5,
            Self::OpenCode => 6,
            Self::Copilot => 7,
            Self::Pi => 8,
            Self::Auggie => 9,
            Self::CursorCli => 10,
            Self::Goose => 11,
            Self::Hermes => 12,
            Self::Vibe => 13,
        }
    }

    /// Decode a u8 FFI tag into a `CliAgent`. Returns `None` for
    /// `BT_CLI_AGENT_NONE` (0) or any unknown tag value (forward-compat
    /// with future agents added to the Rust side before Swift is updated).
    pub fn from_ffi_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Claude),
            2 => Some(Self::Gemini),
            3 => Some(Self::Codex),
            4 => Some(Self::Amp),
            5 => Some(Self::Droid),
            6 => Some(Self::OpenCode),
            7 => Some(Self::Copilot),
            8 => Some(Self::Pi),
            9 => Some(Self::Auggie),
            10 => Some(Self::CursorCli),
            11 => Some(Self::Goose),
            12 => Some(Self::Hermes),
            13 => Some(Self::Vibe),
            _ => None,
        }
    }

    /// Resolve the command line to a known CLI agent, or `None`.
    ///
    /// Detection steps (mirrors Warp):
    ///   1. Trim leading whitespace.
    ///   2. Skip leading env-var assignments (`FOO=1 BAR=2 claude` →
    ///      `claude`). A token counts as an env-var assignment iff
    ///      it matches `[A-Z_][A-Z0-9_]*=…`.
    ///   3. Match the first remaining token's *basename* against
    ///      every variant's `command_prefix()`. Basename strip lets
    ///      `/usr/local/bin/claude` resolve too.
    ///   4. Special case: `vibe-acp` (Mistral Vibe's ACP-mode
    ///      binary) → `Vibe`.
    ///
    /// Aliases are NOT resolved — we don't have access to the remote
    /// shell's alias table from the iOS app. Users running `cc` as
    /// an alias for `claude` won't be detected; document accordingly.
    pub fn detect(command: &str) -> Option<Self> {
        let mut tokens = command.split_whitespace();
        let first = loop {
            let token = tokens.next()?;
            if Self::is_env_assignment(token) {
                continue;
            }
            break token;
        };
        let basename = first.rsplit('/').next().unwrap_or(first);
        if basename == "vibe-acp" {
            return Some(Self::Vibe);
        }
        Self::all()
            .into_iter()
            .find(|agent| basename == agent.command_prefix())
    }

    fn all() -> [Self; 13] {
        [
            Self::Claude,
            Self::Gemini,
            Self::Codex,
            Self::Amp,
            Self::Droid,
            Self::OpenCode,
            Self::Copilot,
            Self::Pi,
            Self::Auggie,
            Self::CursorCli,
            Self::Goose,
            Self::Hermes,
            Self::Vibe,
        ]
    }

    /// `FOO=bar`, `LC_ALL=C`, `__PRIVATE=1` — single uppercase
    /// identifier followed by `=`. Matches POSIX env-var assignment
    /// syntax conservatively (no lowercase to avoid eating valid
    /// command names that happen to contain `=`).
    fn is_env_assignment(token: &str) -> bool {
        let mut chars = token.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first.is_ascii_uppercase() || first == '_') {
            return false;
        }
        for ch in chars {
            if ch == '=' {
                return true;
            }
            if !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_') {
                return false;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Standalone tag-to-string helpers (used by the C FFI / Swift bridge layer)
// ---------------------------------------------------------------------------

/// Map an FFI CLI-agent tag (1–13) to a human-readable display name.
/// Returns an empty string for `BT_CLI_AGENT_NONE` (0) or any unknown tag.
/// Mirrors the Swift `CLIAgent.displayName` getter.
pub fn cli_agent_display_name(tag: u8) -> &'static str {
    CliAgent::from_ffi_tag(tag)
        .map(CliAgent::display_name)
        .unwrap_or("")
}

/// Map an FFI CLI-agent tag to a brand icon name for the asset catalogue,
/// or `None` when the agent has no dedicated icon. Only Claude and Codex
/// ship branded icons; unknown tags and `BT_CLI_AGENT_NONE` also return
/// `None`. Mirrors the Swift `CLIAgent.iconName` getter.
pub fn cli_agent_icon_name(tag: u8) -> Option<&'static str> {
    CliAgent::from_ffi_tag(tag).and_then(CliAgent::icon_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_simple_invocations() {
        assert_eq!(CliAgent::detect("claude"), Some(CliAgent::Claude));
        assert_eq!(CliAgent::detect("codex"), Some(CliAgent::Codex));
        assert_eq!(CliAgent::detect("gemini --help"), Some(CliAgent::Gemini));
    }

    #[test]
    fn skips_env_assignments() {
        assert_eq!(CliAgent::detect("FOO=1 claude"), Some(CliAgent::Claude));
        assert_eq!(
            CliAgent::detect("LC_ALL=C OPENAI_KEY=sk-x codex --help"),
            Some(CliAgent::Codex)
        );
    }

    #[test]
    fn strips_absolute_path() {
        assert_eq!(
            CliAgent::detect("/opt/homebrew/bin/claude --resume"),
            Some(CliAgent::Claude)
        );
    }

    #[test]
    fn vibe_acp_maps_to_vibe() {
        assert_eq!(CliAgent::detect("vibe-acp serve"), Some(CliAgent::Vibe));
    }

    #[test]
    fn cursor_cli_uses_agent_prefix() {
        assert_eq!(CliAgent::detect("agent run"), Some(CliAgent::CursorCli));
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(CliAgent::detect("ls -la"), None);
        assert_eq!(CliAgent::detect("git status"), None);
        assert_eq!(CliAgent::detect(""), None);
        assert_eq!(CliAgent::detect("   "), None);
    }

    #[test]
    fn lowercase_assignment_is_not_env_var() {
        // `foo=bar baz` — `foo=bar` is NOT an env-var assignment per
        // POSIX style (we require uppercase). Treated as the command
        // name `foo=bar`; no agent matches.
        assert_eq!(CliAgent::detect("foo=bar claude"), None);
    }

    #[test]
    fn display_name_all_variants() {
        let cases = [
            (CliAgent::Claude, "Claude Code"),
            (CliAgent::Gemini, "Gemini CLI"),
            (CliAgent::Codex, "Codex"),
            (CliAgent::Amp, "Amp"),
            (CliAgent::Droid, "Droid"),
            (CliAgent::OpenCode, "OpenCode"),
            (CliAgent::Copilot, "GitHub Copilot"),
            (CliAgent::Pi, "Pi"),
            (CliAgent::Auggie, "Auggie"),
            (CliAgent::CursorCli, "Cursor"),
            (CliAgent::Goose, "Goose"),
            (CliAgent::Hermes, "Hermes"),
            (CliAgent::Vibe, "Mistral Vibe"),
        ];
        for (agent, expected) in &cases {
            assert_eq!(agent.display_name(), *expected, "mismatch for {agent:?}");
        }
    }

    #[test]
    fn icon_name_existing_and_none() {
        assert_eq!(CliAgent::Claude.icon_name(), Some("ClaudeLogo"));
        assert_eq!(CliAgent::Codex.icon_name(), Some("OpenAILogo"));
        // All other variants return None.
        let without_icon = [
            CliAgent::Gemini,
            CliAgent::Amp,
            CliAgent::Droid,
            CliAgent::OpenCode,
            CliAgent::Copilot,
            CliAgent::Pi,
            CliAgent::Auggie,
            CliAgent::CursorCli,
            CliAgent::Goose,
            CliAgent::Hermes,
            CliAgent::Vibe,
        ];
        for agent in &without_icon {
            assert_eq!(agent.icon_name(), None, "expected no icon for {agent:?}");
        }
    }

    #[test]
    fn ffi_tag_roundtrip_for_all() {
        for agent in CliAgent::all() {
            let tag = agent.ffi_tag();
            // Reconstruct from tag via exhaustive match (same order as ffi_tag).
            let reconstructed = match tag {
                1 => CliAgent::Claude,
                2 => CliAgent::Gemini,
                3 => CliAgent::Codex,
                4 => CliAgent::Amp,
                5 => CliAgent::Droid,
                6 => CliAgent::OpenCode,
                7 => CliAgent::Copilot,
                8 => CliAgent::Pi,
                9 => CliAgent::Auggie,
                10 => CliAgent::CursorCli,
                11 => CliAgent::Goose,
                12 => CliAgent::Hermes,
                13 => CliAgent::Vibe,
                _ => unreachable!("unknown tag {tag}"),
            };
            assert_eq!(reconstructed, agent, "tag{tag} -> {agent:?} roundtrip");
        }
    }

    #[test]
    fn display_name_matches_swift_switch() {
        // Regression: every agent's display_name must return Some string.
        for agent in CliAgent::all() {
            let name = agent.display_name();
            assert!(!name.is_empty(), "empty display_name for {agent:?}");
        }
    }

    // -----------------------------------------------------------------------
    // from_ffi_tag tests
    // -----------------------------------------------------------------------

    #[test]
    fn from_ffi_tag_all_known() {
        let cases = [
            (1, CliAgent::Claude),
            (2, CliAgent::Gemini),
            (3, CliAgent::Codex),
            (4, CliAgent::Amp),
            (5, CliAgent::Droid),
            (6, CliAgent::OpenCode),
            (7, CliAgent::Copilot),
            (8, CliAgent::Pi),
            (9, CliAgent::Auggie),
            (10, CliAgent::CursorCli),
            (11, CliAgent::Goose),
            (12, CliAgent::Hermes),
            (13, CliAgent::Vibe),
        ];
        for (tag, expected) in &cases {
            assert_eq!(
                CliAgent::from_ffi_tag(*tag),
                Some(*expected),
                "tag {tag} -> {expected:?}"
            );
        }
    }

    #[test]
    fn from_ffi_tag_zero_and_unknown_return_none() {
        assert_eq!(CliAgent::from_ffi_tag(0), None, "BT_CLI_AGENT_NONE");
        assert_eq!(CliAgent::from_ffi_tag(255), None, "max u8 sentinel");
        assert_eq!(CliAgent::from_ffi_tag(14), None, "future tag #1");
        assert_eq!(CliAgent::from_ffi_tag(100), None, "future tag #2");
    }

    #[test]
    fn from_ffi_tag_is_inverse_of_ffi_tag() {
        for agent in CliAgent::all() {
            let tag = agent.ffi_tag();
            assert_eq!(
                CliAgent::from_ffi_tag(tag),
                Some(agent),
                "ffi_tag() -> from_ffi_tag() roundtrip failed for {agent:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // cli_agent_display_name (standalone, tag-based) tests
    // -----------------------------------------------------------------------

    #[test]
    fn cli_agent_display_name_all_tags() {
        let cases = [
            (1, "Claude Code"),
            (2, "Gemini CLI"),
            (3, "Codex"),
            (4, "Amp"),
            (5, "Droid"),
            (6, "OpenCode"),
            (7, "GitHub Copilot"),
            (8, "Pi"),
            (9, "Auggie"),
            (10, "Cursor"),
            (11, "Goose"),
            (12, "Hermes"),
            (13, "Mistral Vibe"),
        ];
        for (tag, expected) in &cases {
            assert_eq!(
                cli_agent_display_name(*tag),
                *expected,
                "tag {tag} display_name mismatch"
            );
        }
    }

    #[test]
    fn cli_agent_display_name_unknown_tags() {
        // 0 is BT_CLI_AGENT_NONE; 255 is a wild out-of-range value.
        assert_eq!(cli_agent_display_name(0), "", "BT_CLI_AGENT_NONE");
        assert_eq!(cli_agent_display_name(255), "", "max u8 sentinel");
        assert_eq!(cli_agent_display_name(14), "", "first future tag");
        assert_eq!(cli_agent_display_name(99), "", "distant future tag");
    }

    #[test]
    fn cli_agent_display_name_unknown_tags_are_empty_not_null() {
        // Regression: must NOT return a C NULL or crash — Rust returns "", never None.
        for tag in [0u8, 14, 100, 200, 255] {
            let name = cli_agent_display_name(tag);
            assert_eq!(
                name, "",
                "tag {tag} should produce empty string, got {name:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // cli_agent_icon_name (standalone, tag-based) tests
    // -----------------------------------------------------------------------

    #[test]
    fn cli_agent_icon_name_claude_and_codex() {
        assert_eq!(cli_agent_icon_name(1), Some("ClaudeLogo"));
        assert_eq!(cli_agent_icon_name(3), Some("OpenAILogo"));
    }

    #[test]
    fn cli_agent_icon_name_other_agents() {
        for tag in [2u8, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
            assert_eq!(
                cli_agent_icon_name(tag),
                None,
                "expected no icon for tag {tag}"
            );
        }
    }

    #[test]
    fn cli_agent_icon_name_unknown_tags() {
        // 0 is BT_CLI_AGENT_NONE, 255 is out of range.
        assert_eq!(cli_agent_icon_name(0), None, "BT_CLI_AGENT_NONE");
        assert_eq!(cli_agent_icon_name(255), None, "max u8 sentinel");
        assert_eq!(cli_agent_icon_name(14), None, "first future tag");
    }
}
