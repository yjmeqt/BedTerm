import SwiftUI
import UIKit

/// Inline / alt-screen mode's "open the composer" affordance. Same
/// shadcn chip styling as the rest of the toolbar — icon + label,
/// `ShadcnMutedForeground`, no Liquid Glass capsule, no shadow.
struct ComposePill: View {
    var morphNamespace: Namespace.ID?
    let onTap: () -> Void

    var body: some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            onTap()
        } label: {
            HStack(spacing: 4) {
                Image(systemName: "square.and.pencil")
                    .font(.system(size: 11, weight: .medium))
                Text(verbatim: String(localized: "Compose"))
                    .font(.system(size: 12))
            }
            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .composerMorphID(in: morphNamespace)
        .accessibilityIdentifier("composer.pill")
        .accessibilityLabel("Compose multi-line input")
    }
}

extension View {
    /// Tags this view so its Liquid Glass capsule morphs between
    /// closed (compose pill) and open (composer bar) states inside
    /// the shared `GlassEffectContainer`. With the flat shadcn
    /// styling we kept the morph ID hook so the namespace bridge in
    /// `TerminalScreen` still compiles, but the actual glass material
    /// no longer participates.
    @ViewBuilder
    func composerMorphID(in namespace: Namespace.ID?) -> some View {
        if let namespace {
            self.glassEffectID("composer-capsule", in: namespace)
        } else {
            self
        }
    }
}
