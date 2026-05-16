import SwiftUI

struct DisconnectBanner: View {
    let reason: String
    let onReconnect: () -> Void

    var body: some View {
        HStack {
            Text(reason)
                .font(.callout)
                .lineLimit(2)
            Spacer()
            Button("Reconnect", action: onReconnect)
                .buttonStyle(.borderedProminent)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.regularMaterial)
        .accessibilityIdentifier("disconnect.banner")
    }
}
