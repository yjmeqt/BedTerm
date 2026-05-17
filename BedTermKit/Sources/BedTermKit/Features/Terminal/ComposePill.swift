import SwiftUI
import UIKit

struct ComposePill: View {
    let onTap: () -> Void
    @State private var appeared = false

    var body: some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            onTap()
        } label: {
            Image(systemName: "square.and.pencil")
                .font(.system(size: 20, weight: .medium))
                .frame(width: 48, height: 48)
                .symbolEffect(.bounce, value: appeared)
                .foregroundStyle(Color.primary)
        }
        .buttonStyle(.plain)
        .glassEffect(.regular.interactive(), in: .capsule)
        .accessibilityIdentifier("composer.pill")
        .accessibilityLabel("Compose multi-line input")
        .onAppear { appeared.toggle() }
    }
}
