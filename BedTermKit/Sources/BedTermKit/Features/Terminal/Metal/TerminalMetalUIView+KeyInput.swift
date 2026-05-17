import UIKit

/// Soft-keyboard input. `UIKeyInput` delivers ordinary characters via
/// `insertText` and backspace via `deleteBackward`; combined with the
/// hardware-key handler in `+HardwareKeys.swift`, this gives the Metal
/// terminal the same input surface SwiftTerm provides.
///
/// `UITextInputTraits` configures terminal-friendly keyboard behavior so
/// the OS does not apply IME corrections that would mangle commands.
extension TerminalMetalUIView: UIKeyInput, UITextInputTraits {
    var hasText: Bool { false }

    func insertText(_ text: String) {
        if text == "\n" {
            onSend(Data([0x0D]))
        } else {
            onSend(Data(text.utf8))
        }
    }

    func deleteBackward() {
        onSend(Data([0x7F]))  // DEL — xterm-256color expects 0x7F.
    }

    var autocorrectionType: UITextAutocorrectionType {
        get { .no }
        set { _ = newValue }
    }
    var autocapitalizationType: UITextAutocapitalizationType {
        get { .none }
        set { _ = newValue }
    }
    var spellCheckingType: UITextSpellCheckingType {
        get { .no }
        set { _ = newValue }
    }
    var smartQuotesType: UITextSmartQuotesType {
        get { .no }
        set { _ = newValue }
    }
    var smartDashesType: UITextSmartDashesType {
        get { .no }
        set { _ = newValue }
    }
    var smartInsertDeleteType: UITextSmartInsertDeleteType {
        get { .no }
        set { _ = newValue }
    }
    var keyboardType: UIKeyboardType {
        get { .asciiCapable }
        set { _ = newValue }
    }
    var keyboardAppearance: UIKeyboardAppearance {
        get { .dark }
        set { _ = newValue }
    }
    var returnKeyType: UIReturnKeyType {
        get { .default }
        set { _ = newValue }
    }
    var enablesReturnKeyAutomatically: Bool {
        get { false }
        set { _ = newValue }
    }
}
