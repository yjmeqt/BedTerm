import Foundation

/// Bundled DCS shell-integration snippet. Returns the raw bytes (UTF-8
/// shell script) that the SSH client writes to a remote PTY's stdin once
/// the shell has produced its first byte.
///
/// The script body lives at
/// `Resources/ShellIntegration/bedterm-integration.sh` inside
/// `BedTermKit`. Always reachable via `Bundle.module`; failure to load
/// it means the resource was dropped from the SwiftPM target on build,
/// which is a programmer error rather than a runtime one.
///
/// ## Bootstrap wrapper format — base64 single-liner
///
/// We do **not** ship the body as an inline heredoc the way Warp does.
/// Heredocs require multi-line stdin parsing on the remote zsh; with
/// ZLE engaged, each line is echoed back to the client (we observed
/// the entire 6 KB body bouncing back as "user typing" in our logs),
/// and PS2-suppression alone isn't enough to make the heredoc parse
/// reliably across zsh versions.
///
/// Instead we base64-encode the body and ship a single-line `eval`
/// statement:
///
///   eval "$(printf %s '<base64>' | base64 -d)"
///
/// One line, no continuation prompts, no heredoc terminator hunting,
/// no ZLE multi-line echo. The encoded payload arrives as one logical
/// line of "user input" — ZLE submits it, the shell decodes inline,
/// and `eval` runs the script body in the current shell context.
enum ShellIntegrationScript {
    /// Read the script body. Returns `nil` only on a packaging
    /// accident (the SPM `.copy("ShellIntegrationResources")` resource
    /// declaration missing, the file deleted, etc).
    static func load() -> String? {
        guard
            let url = Bundle.module.url(
                forResource: "bedterm-integration",
                withExtension: "sh",
                subdirectory: "ShellIntegrationResources")
        else {
            return nil
        }
        return try? String(contentsOf: url, encoding: .utf8)
    }

    /// Return the raw script body. The SFTP path in
    /// `CitadelSSHClient+Bootstrap` writes this verbatim to
    /// `~/.cache/bedterm/integration.sh` on the remote, then triggers
    /// a single `source` line on the PTY — no inline wrapper needed.
    static func bootstrapPayload() -> String? {
        load()
    }
}
