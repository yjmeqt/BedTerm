@preconcurrency import Citadel
import Foundation
@preconcurrency import NIOCore
import os

private let bootstrapLog = Logger(
    subsystem: "com.applovin.yi.bedterm", category: "ssh.bootstrap")

extension CitadelSSHClient {
    /// Install the bundled shell-integration script on the remote
    /// host and source it from the user's interactive shell.
    ///
    /// We do not push the script body via the PTY's stdin. Multiple
    /// failed attempts (raw heredoc, base64 single-liner, chunked
    /// base64) all hit ZLE buffer / syntax-highlighting issues on
    /// real-world zsh setups — the bytes either echo back to the
    /// user, blow past ZLE's per-line limit (bell spam), or trigger
    /// the user's `zsh-syntax-highlighting` to render each char as
    /// it arrives. Robustness is not achievable on the stdin path.
    ///
    /// Instead we open the SFTP subsystem (every default sshd on
    /// macOS / Linux / Synology has it on), write the body to
    /// `~/.cache/bedterm/integration.sh`, close SFTP, then send a
    /// single `source <path>` line on the PTY. One short command
    /// through ZLE — no buffer overflow, nothing to echo.
    static func pushBootstrap(
        payload: String?, writer: TTYStdinWriter,
        ssh: Citadel.SSHClient
    ) async {
        bootstrapLog.log(
            "pushBootstrap entered, payload nil=\(payload == nil, privacy: .public) len=\(payload?.count ?? -1, privacy: .public)"
        )
        guard let payload, !payload.isEmpty,
            let bytes = payload.data(using: .utf8)
        else {
            bootstrapLog.log("pushBootstrap early return (nil or empty)")
            return
        }
        // Let the prompt settle.
        try? await Task.sleep(nanoseconds: 200_000_000)

        // 1. Upload script via SFTP.
        let remoteDir = ".cache/bedterm"
        let remoteFile = "\(remoteDir)/integration.sh"
        do {
            let sftp = try await ssh.openSFTP()
            bootstrapLog.log("sftp opened")
            // mkdir is silently allowed to fail if the dir already exists.
            do {
                try await sftp.createDirectory(atPath: remoteDir)
            } catch {
                bootstrapLog.log(
                    "mkdir \(remoteDir, privacy: .public) skipped (\(String(describing: error), privacy: .public))"
                )
            }
            try await sftp.withFile(
                filePath: remoteFile,
                flags: [.write, .create, .truncate]
            ) { file in
                var buffer = ByteBufferAllocator().buffer(capacity: bytes.count)
                buffer.writeBytes(bytes)
                try await file.write(buffer)
            }
            try? await sftp.close()
            bootstrapLog.log(
                "sftp wrote \(remoteFile, privacy: .public) \(bytes.count, privacy: .public) bytes"
            )
        } catch {
            bootstrapLog.error(
                "sftp upload failed: \(String(describing: error), privacy: .public)"
            )
            return
        }

        // 2. Source the file from the interactive shell.
        // Leading space → `hist_ignore_space` keeps it out of history
        // (most users have it set by default; harmless if not).
        let sourceLine = Data(" source ~/\(remoteFile)\n".utf8)
        try? await writer.write(ByteBuffer(bytes: sourceLine))
        bootstrapLog.log("sourced ~/\(remoteFile, privacy: .public)")
    }
}
