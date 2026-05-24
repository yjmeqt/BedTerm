import SwiftUI

/// Full-bleed scrim shown while `TerminalSession.state == .connecting`.
/// Gives the user a visible "we're still trying" cue and a Cancel button so
/// they can back out instead of staring at an empty Metal view while the SSH
/// handshake hangs on an unreachable host.
struct ConnectingOverlay: View {
    let host: String
    let onCancel: () -> Void

    var body: some View {
        ZStack {
            Color.black.opacity(0.35)
                .ignoresSafeArea()

            VStack(spacing: 16) {
                ProgressView()
                    .controlSize(.large)
                    .tint(Color("ShadcnPrimary", bundle: .module))
                Text("Connecting to \(host)…")
                    .font(.callout)
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                Button(role: .cancel, action: onCancel) {
                    Text("Cancel")
                        .frame(minWidth: 96)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("terminal.connecting.cancel")
            }
            .padding(.horizontal, 28)
            .padding(.vertical, 24)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .accessibilityIdentifier("terminal.connecting.overlay")
    }
}
