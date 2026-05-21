import Foundation

/// Bundled OSC 133 shell-integration snippet. Returns the raw bytes (UTF-8
/// shell script) that callers wrap in a heredoc and write to a remote SSH
/// session's stdin.
///
/// The script lives at `Resources/ShellIntegration/bedterm-integration.sh`
/// inside `BedTermKit`. Always reachable via `Bundle.module`; failure to load
/// it means the resource was dropped from the SwiftPM target on build, which
/// is a programmer error rather than a runtime one.
enum ShellIntegrationScript {
    /// The terminator pattern used by the heredoc wrapper. Picked to be
    /// improbable in any shell script we'd ever bundle; matching it inside
    /// the script body would prematurely terminate the heredoc on the remote
    /// side. The script is reviewed manually before each ship — keep it out
    /// of the source.
    static let heredocSentinel = "BEDTERM_SHELL_INTEGRATION_EOF"

    /// Read the script bytes. Returns `nil` only on a packaging accident
    /// (the SPM `.copy("ShellIntegrationResources")` resource declaration
    /// missing, the file deleted, etc).
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

    /// Build the exact byte sequence pushed into the remote shell: a single
    /// `eval "$(cat <<'SENTINEL' … SENTINEL)"` line that sources the script
    /// inline. The leading space relies on `HISTCONTROL=ignorespace` (which
    /// the wrapper sets first) to keep the bootstrap out of shell history.
    ///
    /// Returns `nil` if the resource can't be loaded (see `load()`).
    static func bootstrapPayload() -> String? {
        guard let body = load() else { return nil }
        return """
             HISTCONTROL=ignorespace eval "$(cat <<'\(heredocSentinel)'
            \(body)
            \(heredocSentinel)
            )"

            """
    }
}
