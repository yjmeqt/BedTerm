import SwiftUI
import UIKit

struct ComposerBar: View {
    @Bindable var controller: ComposerController
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 8) {
            ZStack(alignment: .topLeading) {
                TextEditor(text: $controller.text)
                    .focused($focused)
                    .font(.system(.body, design: .monospaced))
                    .scrollContentBackground(.hidden)
                    .autocorrectionDisabled(true)
                    .textInputAutocapitalization(.never)
                    .keyboardType(.asciiCapable)
                    .frame(minHeight: 44, maxHeight: 240)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                if controller.text.isEmpty {
                    Text("Compose your message…")
                        .font(.system(.body, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 16)
                        .allowsHitTesting(false)
                }
            }
            .background {
                RoundedRectangle(cornerRadius: 18, style: .continuous)
                    .fill(.clear)
                    .glassEffect(.regular, in: .rect(cornerRadius: 18))
            }

            HStack(spacing: 8) {
                Button {
                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    controller.cancel()
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 14, weight: .semibold))
                        .frame(width: 36, height: 36)
                        .foregroundStyle(Color.primary)
                }
                .buttonStyle(.plain)
                .glassEffect(.regular.interactive(), in: .capsule)
                .accessibilityIdentifier("composer.cancel")
                .accessibilityLabel("Discard draft")

                Spacer()

                Button {
                    UINotificationFeedbackGenerator().notificationOccurred(.success)
                    controller.submit()
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "paperplane.fill")
                        Text("Send").font(.system(size: 14, weight: .semibold))
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .foregroundStyle(.white)
                }
                .buttonStyle(.plain)
                .glassEffect(
                    .regular.tint(.accentColor).interactive(),
                    in: .capsule
                )
                .disabled(controller.text.isEmpty)
                .opacity(controller.text.isEmpty ? 0.5 : 1.0)
                .accessibilityIdentifier("composer.send")
                .accessibilityLabel("Send to terminal")
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .onAppear { focused = true }
    }
}
