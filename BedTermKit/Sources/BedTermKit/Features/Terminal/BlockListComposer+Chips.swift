import SwiftUI
import UIKit

/// Footer chip helpers — kept in an extension so `BlockListComposer.swift`
/// stays under the file_length budget. Pure view builders, no state of
/// their own.
extension BlockListComposer {
    var chipDivider: some View {
        Rectangle()
            .fill(Color("ShadcnBorder", bundle: .module).opacity(0.6))
            .frame(width: 1, height: 18)
            .padding(.horizontal, 6)
    }

    func chromeChip(
        icon: String, accent: Bool, flipped: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.system(size: 12, weight: .medium))
                .scaleEffect(y: flipped ? -1 : 1)
                .foregroundStyle(
                    accent
                        ? Color.accentColor
                        : Color("ShadcnMutedForeground", bundle: .module)
                )
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    func footerChip(
        icon: String,
        label: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.system(size: 11, weight: .medium))
                Text(verbatim: label)
                    .font(.system(size: 12))
            }
            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// KeyBarController-backed chip for the raw-PTY modifier keys
    /// (Esc / Ctrl / Tab) now living in the composer footer row instead
    /// of in the separate KeyBar below. `highlighted` swaps the chip
    /// text + icon to `accentColor` (used by the Ctrl latch state).
    func keyChip(
        _ tap: KeyTap, icon: String, label: String, id: String,
        highlighted: Bool = false
    ) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: highlighted ? .medium : .light)
                .impactOccurred()
            keyBar.handle(tap)
        } label: {
            HStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.system(size: 11, weight: .medium))
                Text(verbatim: label)
                    .font(.system(size: 12))
            }
            .foregroundStyle(
                highlighted
                    ? Color.accentColor
                    : Color("ShadcnMutedForeground", bundle: .module)
            )
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("composer.\(id)")
    }
}
