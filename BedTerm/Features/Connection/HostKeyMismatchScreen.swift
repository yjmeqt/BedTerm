import SwiftUI

struct HostKeyMismatchScreen: View {
    let stored: String
    let remote: String
    let host: String
    let port: Int
    let onTrust: () -> Void
    let onReject: () -> Void

    @State private var confirmTrust = false
    private let hostKeyStore = HostKeyStore()

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Label("Host key changed", systemImage: "exclamationmark.triangle.fill")
                .font(.title2)
                .foregroundStyle(.orange)

            Text(
                "\(host):\(port) presented a different host key than we trusted last time. "
                    + "This could mean the server was reinstalled — or that someone is intercepting the connection."
            )
            .font(.callout)

            VStack(alignment: .leading, spacing: 8) {
                Text("Stored fingerprint").font(.caption).foregroundStyle(.secondary)
                Text(stored).font(.system(.body, design: .monospaced))
                Text("Remote fingerprint").font(.caption).foregroundStyle(.secondary)
                Text(remote).font(.system(.body, design: .monospaced))
            }
            .padding()
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))

            Spacer()

            Button("Reject and go back") { onReject() }
                .buttonStyle(.bordered)
                .frame(maxWidth: .infinity)

            Button(role: .destructive) {
                confirmTrust = true
            } label: {
                Text("Trust the new key").frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("mismatch.trust")
        }
        .padding()
        .navigationTitle("Security warning")
        .confirmationDialog(
            "Overwrite stored host key for \(host):\(port)?",
            isPresented: $confirmTrust, titleVisibility: .visible
        ) {
            Button("Trust new key", role: .destructive) {
                try? hostKeyStore.store(fingerprint: remote, host: host, port: port)
                onTrust()
            }
            Button("Cancel", role: .cancel) {}
        }
    }
}
