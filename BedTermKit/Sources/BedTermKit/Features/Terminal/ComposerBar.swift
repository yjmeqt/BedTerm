import SwiftUI
import UIKit

struct ComposerBar: View {
    @Bindable var controller: ComposerController
    @FocusState private var focused: Bool

    var body: some View {
        ZStack(alignment: .topLeading) {
            VStack(spacing: 0) {
                TextEditor(text: $controller.text)
                    .focused($focused)
                    .font(.system(.body, design: .monospaced))
                    .scrollContentBackground(.hidden)
                    .autocorrectionDisabled(true)
                    .textInputAutocapitalization(.never)
                    .padding(.leading, 4)
                    .padding(.top, -6)
                    .padding(.bottom, 0)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

                HStack {
                    Button {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        controller.cancel()
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 11, weight: .semibold))
                            .frame(width: 28, height: 28)
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
                        Image(systemName: "arrow.up")
                            .font(.system(size: 14, weight: .bold))
                            .frame(width: 30, height: 30)
                            .foregroundStyle(.white)
                    }
                    .buttonStyle(.plain)
                    .background(
                        Circle().fill(
                            controller.text.isEmpty
                                ? Color.accentColor.opacity(0.35)
                                : Color.accentColor
                        )
                    )
                    .clipShape(Circle())
                    .disabled(controller.text.isEmpty)
                    .accessibilityIdentifier("composer.send")
                    .accessibilityLabel("Send to terminal")
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)

            if controller.text.isEmpty {
                Text("Compose your message…")
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .padding(.leading, 22)
                    .padding(.top, 14)
                    .allowsHitTesting(false)
            }
        }
        .frame(height: 112)
        .background {
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .fill(.clear)
                .glassEffect(.regular, in: .rect(cornerRadius: 22))
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 6)
        .onAppear {
            UIApplication.shared.sendAction(
                #selector(UIResponder.resignFirstResponder),
                to: nil, from: nil, for: nil
            )
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                focused = true
            }
        }
    }
}
