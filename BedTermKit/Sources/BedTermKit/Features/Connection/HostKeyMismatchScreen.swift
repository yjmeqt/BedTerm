import SwiftUI

struct HostKeyMismatchReviewSheet: View {
    let mismatch: HostsViewModel.PendingMismatch
    let onTrust: () -> Void
    let onReject: () -> Void

    @State private var confirmTrust = false
    private let hostKeyStore = HostKeyStore()

    var body: some View {
        VStack(spacing: 0) {
            Capsule()
                .fill(Color("ShadcnBorder", bundle: .module))
                .frame(width: 36, height: 4)
                .padding(.top, 8)
                .padding(.bottom, 16)

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    HStack(spacing: 10) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                            .font(.title3)
                        Text("Host key changed")
                            .font(.title3.weight(.semibold))
                            .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                    }

                    Text(warningBody)
                        .font(.subheadline)
                        .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        .fixedSize(horizontal: false, vertical: true)

                    VStack(alignment: .leading, spacing: 12) {
                        fingerprintBlock(
                            label: String(localized: "Saved fingerprint"),
                            value: mismatch.stored
                        )
                        fingerprintBlock(
                            label: String(localized: "Remote fingerprint"),
                            value: mismatch.remote
                        )
                    }
                    .padding(12)
                    .background(Color("ShadcnBackground", bundle: .module))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 16)
            }

            VStack(spacing: 8) {
                Button(role: .destructive) {
                    confirmTrust = true
                } label: {
                    Text("Trust New Key")
                        .font(.callout.weight(.medium))
                        .foregroundStyle(Color("ShadcnPrimaryForeground", bundle: .module))
                        .frame(maxWidth: .infinity)
                        .frame(height: 44)
                        .background(Color("ShadcnPrimary", bundle: .module))
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("mismatch.trust")

                Button(action: onReject) {
                    Text("Reject")
                        .font(.callout.weight(.medium))
                        .foregroundStyle(Color("ShadcnDestructive", bundle: .module))
                        .frame(maxWidth: .infinity)
                        .frame(height: 44)
                        .overlay(
                            RoundedRectangle(cornerRadius: 8)
                                .stroke(Color("ShadcnDestructive", bundle: .module), lineWidth: 1)
                        )
                }
                .buttonStyle(.plain)
            }
            .padding(20)
        }
        .background(Color("ShadcnCard", bundle: .module))
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.hidden)
        .confirmationDialog(
            String(localized: "Overwrite stored host key for \(mismatch.host):\(mismatch.port)?"),
            isPresented: $confirmTrust,
            titleVisibility: .visible
        ) {
            Button(String(localized: "Trust new key"), role: .destructive) {
                try? hostKeyStore.store(
                    fingerprint: mismatch.remote,
                    host: mismatch.host,
                    port: mismatch.port
                )
                onTrust()
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        }
    }

    private var warningBody: String {
        String(
            localized: """
                \(mismatch.host):\(mismatch.port) presented a different host key than we trusted \
                last time. This could mean the server was reinstalled — or that someone is \
                intercepting the connection.
                """
        )
    }

    private func fingerprintBlock(label: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.caption)
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            Text(value)
                .font(.system(.footnote, design: .monospaced))
                .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}
