#if DEBUG
    import SwiftUI

    extension HostsScreen {
        @ViewBuilder
        func debugTerminalScreen(for selection: DebugTTYProgramSelection) -> some View {
            let client: any SSHClient = {
                switch selection {
                case .echoShell: return RustMockTTYClient(program: .echoShell)
                case .vimLite: return RustMockTTYClient(program: .vimLite)
                case .rawSink: return RustMockTTYClient(program: .rawSink)
                case .replay(let preset):
                    // The cast bytes are embedded into the Rust rlib via
                    // include_str!; the FFI's `preset` opt picks which one.
                    let opts = "{\"preset\":\"\(preset)\"}"
                    return RustMockTTYClient(program: .replay, opts: opts)
                }
            }()
            TerminalScreen(debugClient: client) {
                if !path.isEmpty {
                    path.removeLast()
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
                    entry("Replay: vim", route: .replay(preset: "vim"))
                    Divider().padding(.leading, 16)
                    entry("Replay: codex", route: .replay(preset: "codex"))
                    Divider().padding(.leading, 16)
                    entry("Replay: claude", route: .replay(preset: "claude"))
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
