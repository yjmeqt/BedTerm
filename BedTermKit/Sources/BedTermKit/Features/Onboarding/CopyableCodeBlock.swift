import SwiftUI
import UIKit

/// A styled, copyable terminal command block for tutorial screens.
/// Renders the command in monospace with a copy button and a "Terminal"
/// attribution label. Uses adaptive materials matching the onboarding cards.
struct CopyableCodeBlock: View {
    let command: String

    @State private var copied = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Terminal")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Spacer()

                Button {
                    UIPasteboard.general.string = command
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                        copied = false
                    }
                } label: {
                    Label(
                        copied ? String(localized: "Copied") : String(localized: "Copy"),
                        systemImage: copied ? "checkmark" : "doc.on.doc"
                    )
                    .font(.caption)
                    .labelStyle(.iconOnly)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)

            Divider()

            Text(command)
                .font(.system(.subheadline, design: .monospaced))
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(.regularMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(.secondary.opacity(0.15), lineWidth: 1)
        )
    }
}
