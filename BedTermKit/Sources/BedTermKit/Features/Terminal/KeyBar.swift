import SwiftUI
import UIKit

struct KeyBar: View {
    @Bindable var controller: KeyBarController
    var keyboardShown: Bool = true
    var dpadOpen: Bool = false
    var onToggleKeyboard: (() -> Void)?
    var onToggleDpad: (() -> Void)?

    private var showsKeyboardToggle: Bool {
        // Hide the dismiss toggle while a hardware keyboard is attached —
        // the software keyboard isn't presented, so the toggle would be a
        // no-op. Per R15.hardware_keyboard_hides_toggle.
        onToggleKeyboard != nil && !HardwareKeyboardObserver.shared.isAttached
    }

    private var showsDpadToggle: Bool { onToggleDpad != nil }

    private var capsuleWidth: CGFloat {
        // 168 = three R3 keys (esc/ctrl/tab) at ~56pt each.
        // +1 divider + 48 per trailing toggle (kbd-dismiss, dpad).
        var width: CGFloat = 168
        if showsKeyboardToggle { width += 1 + 48 }
        if showsDpadToggle { width += (showsKeyboardToggle ? 0 : 1) + 48 }
        return width
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
            if showsKeyboardToggle || showsDpadToggle {
                Rectangle()
                    .fill(Color.primary.opacity(0.18))
                    .frame(width: 1, height: 22)
            }
            if showsDpadToggle, let onToggleDpad {
                dpadKey(onToggle: onToggleDpad)
            }
            if showsKeyboardToggle, let onToggleKeyboard {
                dismissKey(onToggle: onToggleKeyboard)
            }
        }
        .frame(width: capsuleWidth, height: 44)
        .glassEffect(.regular.interactive(), in: .capsule)
    }

    private func dpadKey(onToggle: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            onToggle()
        } label: {
            Image(systemName: dpadOpen ? "dpad.fill" : "dpad")
                .font(.system(size: 18, weight: .medium))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .foregroundStyle(dpadOpen ? Color.accentLabel : Color.primary)
                .background {
                    if dpadOpen {
                        Capsule()
                            .fill(Color.accentColor)
                            .padding(.vertical, 4)
                            .padding(.horizontal, 2)
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(width: 48)
        .accessibilityIdentifier("keybar.dpadtoggle")
        .accessibilityLabel(dpadOpen ? "Hide direction pad" : "Show direction pad")
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
                .foregroundStyle(highlighted ? Color.accentLabel : Color.primary)
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
