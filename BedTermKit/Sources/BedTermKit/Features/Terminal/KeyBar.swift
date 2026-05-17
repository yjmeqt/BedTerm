import SwiftUI
import UIKit

struct KeyBar: View {
    @Bindable var controller: KeyBarController
    var keyboardShown: Bool = true
    var onToggleKeyboard: (() -> Void)?

    var body: some View {
        HStack(spacing: 0) {
            keyButton("⎋", tap: .esc)
            keyButton("⌃", tap: .ctrl, highlighted: controller.isPending)
            keyButton("⇥", tap: .tab)
            if let onToggleKeyboard {
                Divider()
                    .frame(width: 1, height: 22)
                    .background(Color.primary.opacity(0.18))
                dismissKey(onToggle: onToggleKeyboard)
            }
        }
        .frame(width: onToggleKeyboard == nil ? 168 : 220, height: 44)
        .glassEffect(.regular.interactive(), in: .capsule)
        .padding(.vertical, 6)
    }

    private func dismissKey(onToggle: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            onToggle()
        } label: {
            Image(systemName: "keyboard.chevron.compact.down")
                .font(.system(size: 16, weight: .medium))
                .scaleEffect(y: keyboardShown ? 1 : -1)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .foregroundStyle(Color.primary)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(width: 48)
        .accessibilityIdentifier("keybar.kbtoggle")
        .accessibilityLabel(keyboardShown ? "Hide keyboard" : "Show keyboard")
    }

    private func keyButton(_ label: String, tap: KeyTap, highlighted: Bool = false) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: highlighted ? .medium : .light).impactOccurred()
            controller.handle(tap)
        } label: {
            Text(label)
                .font(.system(size: 20, weight: .medium, design: .monospaced))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .foregroundStyle(highlighted ? Color.white : Color.primary)
                .background {
                    if highlighted {
                        Capsule()
                            .fill(Color.accentColor)
                            .padding(.vertical, 4)
                            .padding(.horizontal, 2)
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("keybar.\(label)")
    }
}
