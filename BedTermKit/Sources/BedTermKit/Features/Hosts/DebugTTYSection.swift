#if DEBUG
    import SwiftUI

    extension HostsScreen {
        /// Mock SSH is a loopback throwaway server. Drop any
        /// previously-stored host fingerprint so a fresh key (first run,
        /// regenerated, server re-keyed) lands as trust-on-first-use
        /// instead of a "Host key changed" dead-end.
        private func mockSSHCredential() -> HostCredential {
            let credential = HostCredential(
                host: "127.0.0.1", port: 2222, username: "test",
                auth: .password("x"))
            HostKeyStore().remove(host: credential.host, port: credential.port)
            return credential
        }

        @ViewBuilder
        func debugTerminalScreen(for selection: DebugTTYProgramSelection) -> some View {
            switch selection {
            case .mockSSH:
                let credential = mockSSHCredential()
                TerminalScreen(
                    debugClient: CitadelSSHClient(),
                    credential: credential
                ) {
                    if !path.isEmpty { path.removeLast() }
                }
            default:
                let client: any SSHClient = {
                    switch selection {
                    case .echoShell: return RustMockTTYClient(program: .echoShell)
                    case .vimLite: return RustMockTTYClient(program: .vimLite)
                    case .rawSink: return RustMockTTYClient(program: .rawSink)
                    case .replay(let fixture):
                        let bundleURL = Bundle.main.url(
                            forResource: fixture,
                            withExtension: "cast",
                            subdirectory: "DebugFixtures"
                        )
                        let opts: String? = bundleURL.map { url in
                            "{\"cast_path\":\"\(url.path)\"}"
                        }
                        return RustMockTTYClient(program: .replay, opts: opts)
                    case .mockSSH:
                        preconditionFailure("handled above")
                    }
                }()
                TerminalScreen(debugClient: client) {
                    if !path.isEmpty { path.removeLast() }
                }
            }
        }
    }

    /// Debug-only entry points for the Rust mock TTY. Each row pushes
    /// `AppRoute.debugTerminal(…)` so the HostsScreen can route to a terminal
    /// without going through the host credential form.
    struct DebugTTYSection: View {
        @Binding var path: NavigationPath

        var body: some View {
            VStack(alignment: .leading, spacing: 8) {
                Text(verbatim: "Debug TTY")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 16)

                VStack(spacing: 0) {
                    entry("Echo Shell", route: .echoShell)
                    Divider().padding(.leading, 16)
                    entry("Vim-Lite", route: .vimLite)
                    Divider().padding(.leading, 16)
                    entry("Replay: vim", route: .replay(fixture: "vim-edit"))
                    Divider().padding(.leading, 16)
                    entry("Replay: codex", route: .replay(fixture: "codex-tui"))
                    Divider().padding(.leading, 16)
                    entry("Replay: claude", route: .replay(fixture: "claude-code"))
                    Divider().padding(.leading, 16)
                    entry("Raw Sink", route: .rawSink)
                }
                .background(Color("ShadcnCard", bundle: .module))
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .overlay(
                    RoundedRectangle(cornerRadius: 12)
                        .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
                )
                .padding(.horizontal, 16)
            }
        }

        @ViewBuilder
        private func entry(_ label: String, route: DebugTTYProgramSelection) -> some View {
            Button {
                path.append(AppRoute.debugTerminal(route))
            } label: {
                HStack {
                    Text(verbatim: label)
                        .foregroundStyle(.primary)
                    Spacer()
                    Image(systemName: "chevron.right")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.tertiary)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .buttonStyle(.plain)
        }
    }
#endif
