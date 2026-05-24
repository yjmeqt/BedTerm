import Foundation

/// Snapshot of the prompt-context chips shown above the block-list
/// composer's input. `host` falls out of the connection credential;
/// `cwd` / `gitBranch` track the most recent `Precmd` event (mirrored
/// observable on `BlockStore.latestPwd` / `latestGitBranch`);
/// `lastExitCode` is the previous sealed block's exit.
public struct PromptContext: Equatable, Sendable {
    public var host: String?
    public var cwd: String?
    public var lastExitCode: Int32?
    public var gitBranch: String?

    public init(
        host: String? = nil,
        cwd: String? = nil,
        lastExitCode: Int32? = nil,
        gitBranch: String? = nil
    ) {
        self.host = host
        self.cwd = cwd
        self.lastExitCode = lastExitCode
        self.gitBranch = gitBranch
    }
}
