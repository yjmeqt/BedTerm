import SwiftUI
import UIKit

/// UITextView wrapped for SwiftUI so the composer can:
/// - report rendered content height (drives the collapsed/expanded morph in R13)
/// - take first responder reliably across container layout changes without losing focus
struct ComposerTextView: UIViewRepresentable {
    @Binding var text: String
    @Binding var contentHeight: CGFloat
    var placeholder: String
    var isFocused: Bool
    var onFocusChange: (Bool) -> Void
    /// When set, pressing Return submits instead of inserting a newline.
    /// Multi-line input is reached via the explicit "newline" toolbar
    /// button (block-list Warp composer). Leaving this `nil` preserves
    /// the default UITextView behaviour (Return inserts `\n`) for the
    /// legacy pill composer.
    var onSubmit: (() -> Void)?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.delegate = context.coordinator
        view.backgroundColor = .clear
        let font = UIFont.monospacedSystemFont(ofSize: UIFont.systemFontSize, weight: .regular)
        view.font = font
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 0
        view.autocorrectionType = .no
        view.autocapitalizationType = .none
        view.smartQuotesType = .no
        view.smartDashesType = .no
        view.smartInsertDeleteType = .no
        view.spellCheckingType = .no
        view.isScrollEnabled = true
        view.alwaysBounceVertical = false
        view.adjustsFontForContentSizeCategory = false
        view.keyboardDismissMode = .none
        view.textColor = UIColor.label
        view.tintColor = .tintColor

        // Placeholder lives inside the text view so it hides synchronously with
        // the first keystroke — a SwiftUI-side placeholder lags by one frame
        // because the binding only updates via the delegate callback.
        let label = UILabel()
        label.font = font
        label.textColor = .secondaryLabel
        label.numberOfLines = 1
        label.translatesAutoresizingMaskIntoConstraints = false
        label.isUserInteractionEnabled = false
        label.text = placeholder
        view.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            label.topAnchor.constraint(equalTo: view.topAnchor)
        ])
        context.coordinator.placeholderLabel = label
        context.coordinator.refreshPlaceholder(in: view)
        return view
    }

    func updateUIView(_ uiView: UITextView, context: Context) {
        context.coordinator.placeholderLabel?.text = placeholder
        if uiView.text != text {
            uiView.text = text
            context.coordinator.refreshPlaceholder(in: uiView)
            DispatchQueue.main.async {
                report(height: rendered(height: uiView))
            }
        }
        if isFocused, !uiView.isFirstResponder {
            DispatchQueue.main.async { uiView.becomeFirstResponder() }
        } else if !isFocused, uiView.isFirstResponder {
            DispatchQueue.main.async { uiView.resignFirstResponder() }
        }
    }

    private func rendered(height view: UITextView) -> CGFloat {
        let width = view.bounds.width
        guard width > 0 else { return view.font?.lineHeight ?? 22 }
        let size = view.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
        return size.height
    }

    private func report(height new: CGFloat) {
        guard abs(contentHeight - new) > 0.5 else { return }
        contentHeight = new
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: ComposerTextView
        weak var placeholderLabel: UILabel?
        init(_ parent: ComposerTextView) { self.parent = parent }

        func refreshPlaceholder(in textView: UITextView) {
            placeholderLabel?.isHidden = !textView.text.isEmpty
        }

        func textView(
            _ textView: UITextView,
            shouldChangeTextIn range: NSRange,
            replacementText text: String
        ) -> Bool {
            if text == "\n", let onSubmit = parent.onSubmit {
                onSubmit()
                return false
            }
            return true
        }

        func textViewDidChange(_ textView: UITextView) {
            refreshPlaceholder(in: textView)
            parent.text = textView.text
            let width = textView.bounds.width
            guard width > 0 else { return }
            let size = textView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
            if abs(parent.contentHeight - size.height) > 0.5 {
                parent.contentHeight = size.height
            }
        }

        func textViewDidBeginEditing(_ textView: UITextView) {
            parent.onFocusChange(true)
        }

        func textViewDidEndEditing(_ textView: UITextView) {
            parent.onFocusChange(false)
        }
    }
}
