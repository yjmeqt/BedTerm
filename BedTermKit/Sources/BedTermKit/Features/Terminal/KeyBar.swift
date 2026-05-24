import SwiftUI
import UIKit

/// Flat shadcn-style toolbar of raw-PTY modifier keys + chrome
/// toggles. Each entry renders as an `icon · label` chip — same
/// rhythm as the BlockListComposer footer chips (Newline / History),
/// so the bottom bar reads as one continuous shadcn surface instead
/// of a floating Liquid Glass capsule.
///
/// Buttons:
///   - Esc / Ctrl / Tab — emit bytes via `KeyBarController`.
///   - Dpad toggle (icon only) — opens the directional pad scrim.
///   - Hide-keyboard toggle (icon only) — dismisses the software
///     keyboard while keeping the toolbar visible.
struct KeyBar: View {
    @Bindable var controller: KeyBarController
    var keyboardShown: Bool = true
    var dpadOpen: Bool = false
    /// Pass `false` in block-list mode — Esc/Ctrl/Tab live on the
    /// composer footer there, so the KeyBar only needs the
    /// dpad/keyboard toggles.
    var showsModifierKeys: Bool = true
    var onToggleKeyboard: (() -> Void)?
    var onToggleDpad: (() -> Void)?

    private var showsKeyboardToggle: Bool {
        // Hide the dismiss toggle while a hardware keyboard is attached —
        // the software keyboard isn't presented, so the toggle would be a
        // no-op (R15.hardware_keyboard_hides_toggle).
        onToggleKeyboard != nil && !HardwareKeyboardObserver.shared.isAttached
    }

    private var showsDpadToggle: Bool { onToggleDpad != nil }

    var body: some View {
        HStack(spacing: 4) {
            if showsModifierKeys {
                chip(
                    .esc, icon: "escape",
                    label: String(localized: "Esc"), id: "esc")
                chip(
                    .ctrl, icon: "control",
                    label: String(localized: "Ctrl"),
                    id: "ctrl", highlighted: controller.isPending)
                chip(
                    .tab, icon: "arrow.right.to.line.compact",
                    label: String(localized: "Tab"), id: "tab")
            }
            if showsDpadToggle, let onToggleDpad {
                dpadChip(onToggle: onToggleDpad)
            }
            if showsKeyboardToggle, let onToggleKeyboard {
                kbdChip(onToggle: onToggleKeyboard)
            }
        }
        .font(.system(size: 12))
    }

    private func dpadChip(onToggle: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            onToggle()
        } label: {
            chipLabel(
                icon: dpadOpen ? "dpad.fill" : "dpad",
                label: nil,
                accent: dpadOpen ? .accentColor : nil)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("keybar.dpadtoggle")
        .accessibilityLabel(dpadOpen ? "Hide direction pad" : "Show direction pad")
    }

    private func kbdChip(onToggle: @escaping () -> Void) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            onToggle()
        } label: {
            // Flip the chevron vertically when the keyboard is hidden
            // so a single glyph reads as either "hide" or "show".
            HStack(spacing: 4) {
                Image(systemName: "keyboard.chevron.compact.down")
                    .font(.system(size: 11, weight: .medium))
                    .scaleEffect(y: keyboardShown ? 1 : -1)
            }
            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("keybar.kbtoggle")
        .accessibilityLabel(keyboardShown ? "Hide keyboard" : "Show keyboard")
    }

    private func chip(
        _ tap: KeyTap, icon: String, label: String, id: String,
        highlighted: Bool = false
    ) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: highlighted ? .medium : .light)
                .impactOccurred()
            controller.handle(tap)
        } label: {
            chipLabel(
                icon: icon, label: label,
                accent: highlighted ? .accentColor : nil)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("keybar.\(id)")
    }

    private func chipLabel(icon: String, label: String?, accent: Color?) -> some View {
        HStack(spacing: 4) {
            Image(systemName: icon)
                .font(.system(size: 11, weight: .medium))
            if let label {
                Text(verbatim: label)
                    .font(.system(size: 12))
            }
        }
        .foregroundStyle(accent ?? Color("ShadcnMutedForeground", bundle: .module))
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .contentShape(Rectangle())
    }
}
