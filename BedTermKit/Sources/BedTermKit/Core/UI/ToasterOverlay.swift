import SwiftUI

public struct ToasterOverlay: View {
    @Environment(\.toaster) private var toaster

    private let maxVisible = 3

    public init() {}

    public var body: some View {
        VStack {
            ZStack(alignment: .top) {
                ForEach(Array(self.visible.enumerated()), id: \.element.id) { index, toast in
                    ToastCard(toast: toast) {
                        self.toaster.dismiss(id: toast.id)
                    }
                    .zIndex(Double(self.visible.count - index))
                    .scaleEffect(self.scale(for: index), anchor: .top)
                    .offset(y: self.offset(for: index))
                    .opacity(self.opacity(for: index))
                    .transition(
                        .asymmetric(
                            insertion: .move(edge: .top).combined(with: .opacity),
                            removal: .opacity.combined(with: .scale(scale: 0.9))
                        )
                    )
                }
            }
            .animation(.spring(response: 0.35, dampingFraction: 0.85), value: self.toaster.toasts.map(\.id))
            .padding(.horizontal, 16)
            .padding(.top, 8)
            Spacer(minLength: 0)
        }
        .allowsHitTesting(!self.toaster.toasts.isEmpty)
    }

    private var visible: [Toaster.Toast] {
        let all = self.toaster.toasts
        return Array(all.suffix(self.maxVisible).reversed())
    }

    private func scale(for index: Int) -> CGFloat {
        switch index {
        case 0: return 1.0
        case 1: return 0.96
        default: return 0.92
        }
    }

    private func offset(for index: Int) -> CGFloat {
        CGFloat(index) * 8
    }

    private func opacity(for index: Int) -> Double {
        switch index {
        case 0: return 1.0
        case 1: return 0.92
        default: return 0.78
        }
    }
}

private struct ToastCard: View {
    let toast: Toaster.Toast
    let onDismiss: () -> Void

    @GestureState private var dragOffset: CGFloat = 0

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            self.icon
            VStack(alignment: .leading, spacing: 2) {
                Text(self.toast.title)
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                if let description = self.toast.description {
                    Text(description)
                        .font(.footnote)
                        .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        .fixedSize(horizontal: false, vertical: true)
                }
                if !self.toast.actions.isEmpty {
                    HStack(spacing: 8) {
                        ForEach(self.toast.actions) { action in
                            Button {
                                action.handler()
                            } label: {
                                Text(action.title)
                                    .font(.footnote.weight(.medium))
                            }
                            .buttonStyle(ToastActionButtonStyle(isDestructive: action.isDestructive))
                        }
                    }
                    .padding(.top, 8)
                }
            }
            Spacer(minLength: 0)
            if !self.toast.isPersistent {
                Button(action: self.onDismiss) {
                    Image(systemName: "xmark")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        .padding(6)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Dismiss"))
            }
        }
        .padding(12)
        .background(Color("ShadcnCard", bundle: .module))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .shadow(color: Color.black.opacity(0.08), radius: 12, y: 4)
        .contentShape(Rectangle())
        // Absorb taps on empty card areas so they don't fall through to
        // toolbar buttons (notably the Add "+") underneath. The DragGesture
        // alone doesn't claim taps, which is what caused the pass-through.
        .onTapGesture {}
        .offset(y: min(self.dragOffset, 0))
        .simultaneousGesture(
            self.toast.isPersistent
                ? nil
                : DragGesture()
                    .updating(self.$dragOffset) { value, state, _ in
                        state = value.translation.height
                    }
                    .onEnded { value in
                        if value.translation.height < -30 {
                            self.onDismiss()
                        }
                    }
        )
    }

    @ViewBuilder
    private var icon: some View {
        let (symbol, color) = self.iconSpec
        Image(systemName: symbol)
            .font(.footnote.weight(.bold))
            .foregroundStyle(.white)
            .frame(width: 18, height: 18)
            .background(color, in: Circle())
    }

    private var iconSpec: (String, Color) {
        switch self.toast.kind {
        case .info:
            return ("info", Color("ShadcnMutedForeground", bundle: .module))
        case .success:
            return ("checkmark", .green)
        case .warning:
            return ("exclamationmark", .orange)
        case .error:
            return ("exclamationmark", Color("ShadcnDestructive", bundle: .module))
        }
    }
}

private struct ToastActionButtonStyle: ButtonStyle {
    let isDestructive: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(.horizontal, 10)
            .frame(height: 28)
            .foregroundStyle(self.foreground(pressed: configuration.isPressed))
            .background(self.background(pressed: configuration.isPressed))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(self.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    private func foreground(pressed: Bool) -> Color {
        if self.isDestructive {
            return Color("ShadcnDestructive", bundle: .module)
        }
        return Color("ShadcnPrimaryForeground", bundle: .module)
            .opacity(pressed ? 0.8 : 1.0)
    }

    private func background(pressed: Bool) -> Color {
        if self.isDestructive {
            return Color.clear
        }
        return Color("ShadcnPrimary", bundle: .module)
            .opacity(pressed ? 0.85 : 1.0)
    }

    private var border: Color {
        if self.isDestructive {
            return Color("ShadcnDestructive", bundle: .module)
        }
        return Color("ShadcnPrimary", bundle: .module)
    }
}
