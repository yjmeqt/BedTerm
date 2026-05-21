import BedTermCoreC
import Foundation

/// Shell-integration boundary event surfaced by the Rust OSC 133 sniffer.
/// Mirrors the FinalTerm 133;A/B/C/D vocabulary; consumers (e.g. a Block
/// view model) reconstruct command lifecycles from a sequence of these.
///
/// `attrs` carries any extension parameters the remote shell shipped — iTerm2
/// adds `user-host=` and `current-dir=`, kitty adds `k=`, our own bundled
/// script adds `cmd=<base64>`, `dur=<millis>`, `cwd=<base64>`. Empty when
/// the integration didn't ship any (third-party scripts often don't). Values
/// are not URL-decoded or base64-decoded; the consumer decides what to do
/// with each known key.
public enum Osc133Event: Sendable, Equatable {
    /// `ESC]133;A[;<attrs>]` — shell is about to draw a prompt.
    case promptStart(attrs: [String: String])
    /// `ESC]133;B[;<attrs>]` — prompt finished; bytes after are the typed
    /// command line.
    case commandStart(attrs: [String: String])
    /// `ESC]133;C[;<attrs>]` — user pressed Return; bytes after are command
    /// output. Our bundled script ships the command text here as `cmd=<b64>`.
    case outputStart(attrs: [String: String])
    /// `ESC]133;D[;<exit>][;<attrs>]` — command finished.
    case commandEnd(exitCode: Int32?, attrs: [String: String])

    /// Decode the C ABI struct. Returns `nil` on an unrecognised discriminator
    /// (should be impossible — Rust only emits the four known values).
    init?(raw: BtOsc133Event) {
        let attrs = Self.parseAttrs(ptr: raw.attrs, len: Int(raw.attrs_len))
        switch raw.kind {
        case UInt8(BT_OSC133_PROMPT_START):
            self = .promptStart(attrs: attrs)
        case UInt8(BT_OSC133_COMMAND_START):
            self = .commandStart(attrs: attrs)
        case UInt8(BT_OSC133_OUTPUT_START):
            self = .outputStart(attrs: attrs)
        case UInt8(BT_OSC133_COMMAND_END):
            self = .commandEnd(
                exitCode: raw.has_exit_code != 0 ? raw.exit_code : nil,
                attrs: attrs
            )
        default:
            return nil
        }
    }

    /// Parse `key=value;key=value` bytes into a dictionary. Returns empty on
    /// `nil` / zero-length input. Malformed entries (missing `=`) are
    /// preserved as keys with an empty value — surprising data is better
    /// surfaced than silently dropped.
    private static func parseAttrs(ptr: UnsafePointer<UInt8>?, len: Int) -> [String: String] {
        guard let ptr, len > 0 else { return [:] }
        // Copy out immediately — the Rust scratch buffer is invalidated by the
        // next mutating call.
        let bytes = UnsafeBufferPointer(start: ptr, count: len)
        guard let payload = String(bytes: bytes, encoding: .utf8) else { return [:] }
        var out: [String: String] = [:]
        for pair in payload.split(separator: ";", omittingEmptySubsequences: true) {
            if let eq = pair.firstIndex(of: "=") {
                let key = String(pair[..<eq])
                let value = String(pair[pair.index(after: eq)...])
                out[key] = value
            } else {
                out[String(pair)] = ""
            }
        }
        return out
    }
}
