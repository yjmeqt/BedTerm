import SwiftUI
import UIKit

struct KeyBar: View {
    @Bindable var controller: KeyBarController
    var keyboardShown: Bool = true
    var onToggleKeyboard: (() -> Void)?

    private var showsToggle: Bool {
        // Hide the dismiss toggle while a hardware keyboard is attached —
        // the software keyboard isn't presented, so the toggle would be a
        // no-op. Per R15.hardware_keyboard_hides_toggle.
        onToggleKeyboard != nil && !HardwareKeyboardObserver.shared.isAttached
    }

    var body: some View {
        HStack(spacing: 0) {
            keyButton(.esc, system: "escape", id: "esc")
            keyButton(
                .ctrl,
                system: "control",
                id: "ctrl",
                highlighted: controller.isPending
            )
            keyButton(.tab, system: "arrow.right.to.line.compact", id: "tab")
            if showsToggle, let onToggleKeyboard {
                Rectangle()
                    .fill(Color.primary.opacity(0.18))
                    .frame(width: 1, height: 22)
                dismissKey(onToggle: onToggleKeyboard)
            }
        }
        .frame(width: showsToggle ? 220 : 168, height: 44)
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

    private func keyButton(
        _ tap: KeyTap,
        system symbolName: String,
        id: String,
        highlighted: Bool = false
    ) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: highlighted ? .medium : .light).impactOccurred()
            controller.handle(tap)
        } label: {
            Image(systemName: symbolName)
                .font(.system(size: 18, weight: .medium))
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
        .accessibilityIdentifier("keybar.\(id)")
    }
}
