import SwiftUI
import UIKit

/// R13 composer bar.
///
/// Geometry rules (R13.composer_collapsed_default … R13.composer_expanded_layout):
/// - Collapsed (single line): the container is a capsule that holds
///   `[ text | ✕ | ↑ ]` in one row. Height fits one line of monospace text plus padding.
/// - Auto-grow: as the buffer wraps past one line the container grows continuously
///   to fit the rendered text, capping at `maxHeight`. Once growth begins the shape
///   morphs from a capsule into a rounded-rect.
/// - Expanded (multi-line): ✕ moves to the top-trailing corner and ↑ sits at the
///   bottom-trailing corner; the text occupies the leading column with right
///   padding reserved for the controls.
struct ComposerBar: View {
    @Bindable var controller: ComposerController
    var morphNamespace: Namespace.ID?
    @State private var contentHeight: CGFloat = 0
    @State private var isFocused: Bool = false

    private let horizontalPadding: CGFloat = 16
    private let verticalPadding: CGFloat = 11
    private let singleLineHeight: CGFloat = 22
    private let maxHeight: CGFloat = 112

    private let sendDiameter: CGFloat = 30
    private let cancelDiameter: CGFloat = 28
    private let edgeInset: CGFloat = 8
    private let inlineGap: CGFloat = 4

    private var trailingTextInset: CGFloat {
        // Always reserve enough width for both controls so the morph never reflows text.
        edgeInset + sendDiameter + inlineGap + cancelDiameter + edgeInset / 2
    }

    private var collapsedHeight: CGFloat { singleLineHeight + verticalPadding * 2 }

    /// Smallest expanded height that fits ✕ at the top-trailing corner and ↑ at
    /// the bottom-trailing corner without them overlapping (two edge insets,
    /// both buttons, and a small breathing gap between them).
    private var minExpandedHeight: CGFloat {
        edgeInset * 2 + cancelDiameter + 6 + sendDiameter
    }

    private var containerHeight: CGFloat {
        let measured = contentHeight + verticalPadding * 2
        let floor = isExpanded ? minExpandedHeight : collapsedHeight
        return min(max(floor, measured), maxHeight)
    }

    private var isExpanded: Bool {
        contentHeight > singleLineHeight + 4
    }

    private var cornerRadius: CGFloat {
        min(containerHeight / 2, 22)
    }

    var body: some View {
        ZStack(alignment: .topLeading) {
            ComposerTextView(
                text: $controller.text,
                contentHeight: $contentHeight,
                placeholder: String(localized: "Compose your message…"),
                isFocused: isFocused,
                onFocusChange: { isFocused = $0 }
            )
            .padding(.leading, horizontalPadding)
            .padding(.trailing, trailingTextInset)
            .padding(.vertical, verticalPadding)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

            // ✕ Cancel: trailing-row inline when collapsed, top-trailing corner when expanded.
            cancelButton
                .padding(
                    .trailing,
                    isExpanded ? edgeInset : edgeInset + sendDiameter + inlineGap
                )
                .padding(.top, isExpanded ? edgeInset : 0)
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: isExpanded ? .topTrailing : .trailing
                )

            // ↑ Send: trailing-row inline when collapsed, bottom-trailing corner when expanded.
            sendButton
                .padding(.trailing, edgeInset)
                .padding(.bottom, isExpanded ? edgeInset : 0)
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: isExpanded ? .bottomTrailing : .trailing
                )
        }
        .frame(maxWidth: .infinity)
        .frame(height: containerHeight)
        // Glass + morph ID applied to the container itself, so the Liquid
        // Glass material sits *behind* the text and buttons rather than as
        // a sibling layer that the GlassEffectContainer promotes in front.
        .glassEffect(.regular, in: .rect(cornerRadius: cornerRadius))
        .composerMorphID(in: morphNamespace)
        .animation(.smooth(duration: 0.22), value: containerHeight)
        .animation(.smooth(duration: 0.22), value: isExpanded)
        .onAppear {
            // SwiftTerm already yielded first-responder via TerminalHostView's
            // `yieldFirstResponder` binding before this view was inserted, so
            // we can grab focus immediately for a same-tick keyboard handoff.
            isFocused = true
        }
    }

    private var cancelButton: some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            controller.cancel()
        } label: {
            Image(systemName: "xmark")
                .font(.system(size: 11, weight: .semibold))
                .frame(width: cancelDiameter, height: cancelDiameter)
                .foregroundStyle(Color.primary)
        }
        .buttonStyle(.plain)
        .glassEffect(.regular.interactive(), in: .capsule)
        .accessibilityIdentifier("composer.cancel")
        .accessibilityLabel("Discard draft")
    }

    private var sendButton: some View {
        Button {
            UINotificationFeedbackGenerator().notificationOccurred(.success)
            controller.submit()
        } label: {
            Image(systemName: "arrow.up")
                .font(.system(size: 14, weight: .bold))
                .frame(width: sendDiameter, height: sendDiameter)
                .foregroundStyle(.white)
        }
        .buttonStyle(.plain)
        .background(
            Circle().fill(
                controller.text.isEmpty
                    ? Color.accentColor.opacity(0.35)
                    : Color.accentColor
            )
        )
        .clipShape(Circle())
        .shadow(
            color: controller.text.isEmpty ? .clear : Color.accentColor.opacity(0.45),
            radius: 8, x: 0, y: 3
        )
        .disabled(controller.text.isEmpty)
        .accessibilityIdentifier("composer.send")
        .accessibilityLabel("Send to terminal")
    }
}
