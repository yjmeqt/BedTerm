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
}
