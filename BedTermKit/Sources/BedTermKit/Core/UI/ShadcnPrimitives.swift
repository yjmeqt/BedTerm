import SwiftUI
import UIKit

/// Card with a header (title + optional description) and a body slot. The
/// header sits flush with the card padding; the body slot gets a 14pt vertical
/// stack so callers can pack `ShadcnField`s without further wrapping.
struct ShadcnCard<Content: View>: View {
    let title: String
    let description: String?
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                if let description {
                    Text(description)
                        .font(.footnote)
                        .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            VStack(spacing: 14) {
                content()
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color("ShadcnCard", bundle: .module))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color("ShadcnBorder", bundle: .module), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

struct ShadcnField<Content: View>: View {
    let label: String
    var help: String?
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Text(label)
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                if let help {
                    Text(help)
                        .font(.caption)
                        .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                }
            }
            content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct ShadcnTextField: View {
    @Binding var text: String
    var keyboard: UIKeyboardType = .default
    var identifier: String?

    var body: some View {
        TextField("", text: $text)
            .keyboardType(keyboard)
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
            .font(.callout)
            .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
            .padding(.horizontal, 12)
            .frame(height: 36)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color("ShadcnInput", bundle: .module), lineWidth: 1)
            )
            .modifier(IdentifierIf(identifier: identifier))
    }
}

struct ShadcnSecureField: View {
    @Binding var text: String
    var placeholder: String = ""
    var contentType: UITextContentType?
    var identifier: String?

    var body: some View {
        SecureField(placeholder, text: $text)
            .font(.callout)
            .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
            .padding(.horizontal, 12)
            .frame(height: 36)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(Color("ShadcnInput", bundle: .module), lineWidth: 1)
            )
            .modifier(ContentTypeIf(type: contentType))
            .modifier(IdentifierIf(identifier: identifier))
    }
}

private struct IdentifierIf: ViewModifier {
    let identifier: String?
    func body(content: Content) -> some View {
        if let id = identifier { content.accessibilityIdentifier(id) } else { content }
    }
}

private struct ContentTypeIf: ViewModifier {
    let type: UITextContentType?
    func body(content: Content) -> some View {
        if let type { content.textContentType(type) } else { content }
    }
}
