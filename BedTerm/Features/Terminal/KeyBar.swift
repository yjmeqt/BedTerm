import SwiftUI
import UIKit

struct KeyBar: View {
    @Bindable var controller: KeyBarController

    var body: some View {
        HStack(spacing: 4) {
            keyButton("⌃", tap: .ctrl, highlighted: controller.isPending)
            keyButton("⇥", tap: .tab)
            keyButton("⎋", tap: .esc)
            keyButton("←", tap: .left)
            keyButton("→", tap: .right)
            keyButton("↑", tap: .up)
            keyButton("↓", tap: .down)
        }
        .padding(.horizontal, 4)
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity)
        .background(.bar)
    }

    private func keyButton(_ label: String, tap: KeyTap, highlighted: Bool = false) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: highlighted ? .medium : .light).impactOccurred()
            controller.handle(tap)
        } label: {
            Text(label)
                .font(.system(size: 18, weight: .medium, design: .monospaced))
                .frame(maxWidth: .infinity, minHeight: 32)
        }
        .buttonStyle(.plain)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(highlighted ? Color.accentColor.opacity(0.35) : Color.gray.opacity(0.15))
        )
        .accessibilityIdentifier("keybar.\(label)")
    }
}
