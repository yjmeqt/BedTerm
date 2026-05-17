import UIKit

/// Hardware-key handling. Maps UIKey events that `UIKeyInput.insertText`
/// cannot deliver (arrows, escape, function keys, Ctrl-combos) to the
/// canonical xterm/VT byte sequence and forwards them through `onSend`.
extension TerminalMetalUIView {

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var handled = false
        for press in presses {
            guard let key = press.key else { continue }
            if let bytes = Self.encode(key: key) {
                onSend(bytes)
                handled = true
            }
        }
        if !handled { super.pressesBegan(presses, with: event) }
    }

    private static let esc: UInt8 = 0x1B

    /// Static map from UIKey.keyCode to the canonical xterm/VT byte sequence.
    static let keyCodeBytes: [UIKeyboardHIDUsage: [UInt8]] = [
        .keyboardUpArrow: [esc, 0x5B, 0x41],
        .keyboardDownArrow: [esc, 0x5B, 0x42],
        .keyboardRightArrow: [esc, 0x5B, 0x43],
        .keyboardLeftArrow: [esc, 0x5B, 0x44],
        .keyboardHome: [esc, 0x5B, 0x48],
        .keyboardEnd: [esc, 0x5B, 0x46],
        .keyboardPageUp: [esc, 0x5B, 0x35, 0x7E],
        .keyboardPageDown: [esc, 0x5B, 0x36, 0x7E],
        .keyboardEscape: [esc],
        .keyboardTab: [0x09],
        .keyboardReturnOrEnter: [0x0D],
        .keyboardDeleteOrBackspace: [0x7F]
    ]

    static func encode(key: UIKey) -> Data? {
        if let ctrlByte = encodeControl(key: key) { return Data([ctrlByte]) }
        return keyCodeBytes[key.keyCode].map { Data($0) }
    }

    static func encodeControl(key: UIKey) -> UInt8? {
        guard key.modifierFlags.contains(.control), key.characters.count == 1,
            let ascii = key.characters.uppercased().unicodeScalars.first?.value,
            ascii >= 0x40, ascii <= 0x5F
        else { return nil }
        return UInt8(ascii - 0x40)
    }
}
