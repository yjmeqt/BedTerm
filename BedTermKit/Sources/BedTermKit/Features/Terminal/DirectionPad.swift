import SwiftUI
import UIKit

/// Floating PS5-style direction pad. Renders four chevron wedges around a
/// central dismiss disc. Direction taps emit `KeyTap.up/.down/.left/.right`
/// through the existing `KeyBarController`; the centre disc closes the pad.
struct DirectionPad: View {
    var onDirection: (KeyTap) -> Void
    var onClose: () -> Void

    private let diameter: CGFloat = 200
    private let centerDiameter: CGFloat = 64

    var body: some View {
        ZStack {
            Circle()
                .fill(.clear)
                .frame(width: diameter, height: diameter)
                .glassEffect(.regular.interactive(), in: .circle)

            directionButton(
                tap: .up,
                system: "chevron.up",
                id: "up",
                offset: CGSize(width: 0, height: -wedgeOffset)
            )
            directionButton(
                tap: .down,
                system: "chevron.down",
                id: "down",
                offset: CGSize(width: 0, height: wedgeOffset)
            )
            directionButton(
                tap: .left,
                system: "chevron.left",
                id: "left",
                offset: CGSize(width: -wedgeOffset, height: 0)
            )
            directionButton(
                tap: .right,
                system: "chevron.right",
                id: "right",
                offset: CGSize(width: wedgeOffset, height: 0)
            )

            closeButton
        }
        .frame(width: diameter, height: diameter)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Direction pad")
    }

    private var wedgeOffset: CGFloat { diameter / 2 - 28 }

    private func directionButton(
        tap: KeyTap,
        system symbolName: String,
        id: String,
        offset: CGSize
    ) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            onDirection(tap)
        } label: {
            Image(systemName: symbolName)
                .font(.system(size: 22, weight: .semibold))
                .foregroundStyle(Color.primary)
                .frame(width: 56, height: 56)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .offset(offset)
        .accessibilityIdentifier("dpad.\(id)")
        .accessibilityLabel(accessibilityLabel(for: tap))
    }

    private var closeButton: some View {
        Button {
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
            onClose()
        } label: {
            ZStack {
                Circle()
                    .fill(Color.primary.opacity(0.12))
                Image(systemName: "xmark")
                    .font(.system(size: 16, weight: .bold))
                    .foregroundStyle(Color.primary)
            }
            .frame(width: centerDiameter, height: centerDiameter)
            .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("dpad.close")
        .accessibilityLabel("Hide direction pad")
    }

    private func accessibilityLabel(for tap: KeyTap) -> String {
        switch tap {
        case .up: "Up arrow"
        case .down: "Down arrow"
        case .left: "Left arrow"
        case .right: "Right arrow"
        default: ""
        }
    }
}
