import SwiftUI
import UIKit

struct ComposePill: View {
    var morphNamespace: Namespace.ID?
    let onTap: () -> Void
    @State private var appeared = false

    var body: some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            onTap()
        } label: {
            Image(systemName: "square.and.pencil")
                .font(.system(size: 20, weight: .medium))
                .frame(width: 44, height: 44)
                .symbolEffect(.bounce, value: appeared)
                .foregroundStyle(Color.primary)
        }
        .buttonStyle(.plain)
        .glassEffect(.regular.interactive(), in: .capsule)
        .composerMorphID(in: morphNamespace)
        .accessibilityIdentifier("composer.pill")
        .accessibilityLabel("Compose multi-line input")
        .onAppear { appeared.toggle() }
    }
}

extension View {
    /// Tags this view so its Liquid Glass capsule morphs between closed (compose pill)
    /// and open (composer bar) states inside the shared `GlassEffectContainer`.
    @ViewBuilder
    func composerMorphID(in namespace: Namespace.ID?) -> some View {
        if let namespace {
            self.glassEffectID("composer-capsule", in: namespace)
        } else {
            self
        }
    }
}
