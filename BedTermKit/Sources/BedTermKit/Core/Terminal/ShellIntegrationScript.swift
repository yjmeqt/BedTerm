import BedTermIOS
import Foundation

/// Bundled DCS shell-integration snippet. Returns the raw bytes (UTF-8
/// shell script) that the SSH client writes to a remote PTY's stdin once
/// the shell has produced its first byte.
///
/// The script body lives in the Rust crate at
/// `rust-core/bedterm-ios/assets/bedterm-integration.sh` and is embedded
/// into the staticlib at build time via `include_bytes!`. Swift fetches
/// the bytes from `.rodata` via [`bt_ios_shell_integration_payload`] —
/// no SwiftPM resource, no `Bundle.module` lookup, no failure mode short
/// of a corrupt binary.
///
/// ## Bootstrap wrapper format — base64 single-liner (historical)
///
/// We do **not** ship the body as an inline heredoc the way Warp does.
/// Heredocs require multi-line stdin parsing on the remote zsh; with
/// ZLE engaged, each line is echoed back to the client (we observed
/// the entire 6 KB body bouncing back as "user typing" in our logs),
/// and PS2-suppression alone isn't enough to make the heredoc parse
/// reliably across zsh versions.
///
/// Instead we base64-encode the body and ship a single-line `eval`
/// statement. One line, no continuation prompts, no heredoc terminator
/// hunting, no ZLE multi-line echo. The encoded payload arrives as one
/// logical line of "user input" — ZLE submits it, the shell decodes
/// inline, and `eval` runs the script body in the current shell
/// context.
enum ShellIntegrationScript {
    /// Read the script body from the embedded staticlib payload. Returns
    /// `nil` only if the embedded bytes aren't valid UTF-8, which would
    /// indicate a corrupt build artefact rather than a runtime
    /// condition.
    static func load() -> String? {
        var length: Int = 0
        guard let ptr = bt_ios_shell_integration_payload(&length), length > 0 else {
            return nil
        }
        let buffer = UnsafeBufferPointer(start: ptr, count: length)
        return String(bytes: buffer, encoding: .utf8)
    }

    /// Return the raw script body. `RusshSSHClient` injects this payload
    /// after the remote shell produces its first byte.
    static func bootstrapPayload() -> String? {
        load()
    }
}
