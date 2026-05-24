import Foundation
import os

/// DEBUG-only byte tracer. Streams every chunk read/written on the SSH
/// channel to the system log with subsystem `com.applovin.yi.bedterm`,
/// category `ssh.bytes`. View live from a Mac with:
///
///   log stream --predicate 'subsystem == "com.applovin.yi.bedterm"
///                           AND category == "ssh.bytes"'
///                --style ndjson
///
/// (or open Console.app and search for `ssh.bytes`.)
let sshBytesLog = Logger(
    subsystem: "com.applovin.yi.bedterm", category: "ssh.bytes")

func logChunk(_ direction: String, _ data: Data) {
    #if DEBUG
        // Encode at most 256 bytes of preview so the log line stays
        // readable; full length is reported separately.
        let preview = data.prefix(256)
        let ascii = preview.map { byte -> String in
            switch byte {
            case 0x20...0x7E: return String(UnicodeScalar(byte))
            case 0x0A: return "\\n"
            case 0x0D: return "\\r"
            case 0x09: return "\\t"
            case 0x1B: return "\\e"
            default: return String(format: "\\x%02x", byte)
            }
        }.joined()
        let dirPub = direction
        let lenPub = data.count
        let asciiPub = ascii
        sshBytesLog.log(
            "\(dirPub, privacy: .public) len=\(lenPub, privacy: .public) bytes=\(asciiPub, privacy: .public)"
        )
    #endif
}
