@preconcurrency import Citadel
import Foundation
@preconcurrency import NIOCore

extension CitadelSSHClient {
    /// Write the bootstrap payload (e.g. our OSC 133 shell-integration
    /// heredoc) into the remote channel before yielding control to the user.
    /// Failures are swallowed: the session still works without integration,
    /// just without block markers.
    static func pushBootstrap(
        payload: String?, writer: TTYStdinWriter
    ) async {
        guard let payload, !payload.isEmpty,
            let bytes = payload.data(using: .utf8)
        else { return }
        try? await writer.write(ByteBuffer(bytes: bytes))
    }
}
