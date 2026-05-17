import SwiftUI
import UIKit

struct KeyBar: View {
    @Bindable var controller: KeyBarController

    var body: some View {
        HStack(spacing: 0) {
            keyButton("⎋", tap: .esc)
            keyButton("⌃", tap: .ctrl, highlighted: controller.isPending)
            keyButton("⇥", tap: .tab)
        }
        .frame(width: 168, height: 44)
        .glassEffect(.regular.interactive(), in: .capsule)
        .padding(.vertical, 6)
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
