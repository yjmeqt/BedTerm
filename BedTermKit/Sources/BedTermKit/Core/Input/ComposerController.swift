import Foundation
import Observation

@MainActor
@Observable
public final class ComposerController {
    public var text: String = ""
    public private(set) var isOpen: Bool = false
    /// When true the composer has handed control to a running command —
    /// the editor still displays the just-submitted command text for
    /// reference, but key events bypass the buffer and stream straight
    /// to the PTY as stdin. Mirrors Warp's behaviour where
    /// `should_write_typed_chars_to_pty` flips the moment a block
    /// `started()` (see app/src/terminal/view.rs:8186 in warp).
    public private(set) var isPassthrough: Bool = false

    private let send: (Data) -> Void
    private let isBracketedPasteActive: () -> Bool
    private let returnFocusToTerminal: () -> Void

    public init(
        send: @escaping (Data) -> Void,
        isBracketedPasteActive: @escaping () -> Bool,
        returnFocusToTerminal: @escaping () -> Void = {}
    ) {
        self.send = send
        self.isBracketedPasteActive = isBracketedPasteActive
        self.returnFocusToTerminal = returnFocusToTerminal
    }

    public func open() {
        isOpen = true
    }

    public func cancel() {
        // Hand first-responder back to the terminal *before* flipping isOpen, so
        // the system keyboard sees a same-tick responder handoff (no dismiss).
        returnFocusToTerminal()
        text = ""
        isOpen = false
        isPassthrough = false
    }

    public func submit() {
        guard !text.isEmpty else { return }
        let trailingReturn = Data([0x0D])  // CR — runs the last line
        let bytes: Data
        if isBracketedPasteActive() {
            let prefix = Data([0x1B, 0x5B, 0x32, 0x30, 0x30, 0x7E])  // ESC [ 200~
            let suffix = Data([0x1B, 0x5B, 0x32, 0x30, 0x31, 0x7E])  // ESC [ 201~
            bytes = prefix + Data(text.utf8) + suffix + trailingReturn
        } else {
            bytes = Data(text.replacingOccurrences(of: "\n", with: "\r").utf8) + trailingReturn
        }
        send(bytes)
        // Keep the submitted command text visible and lock the editor
        // until the running block ends. Subsequent keystrokes will be
        // forwarded to the PTY as stdin via `sendPassthrough`.
        isPassthrough = true
    }

    /// Called when the running command finishes (no block is running
    /// anymore). Clears the now-stale command text and re-enables the
    /// editor for the next command.
    public func endPassthrough() {
        guard isPassthrough else { return }
        text = ""
        isPassthrough = false
    }

    /// Forward a user keystroke directly to the PTY while passthrough
    /// is active. Returns true if the character was consumed.
    @discardableResult
    public func sendPassthrough(_ chars: String) -> Bool {
        guard isPassthrough, !chars.isEmpty else { return false }
        let bytes = chars == "\n" ? Data([0x0D]) : Data(chars.utf8)
        send(bytes)
        return true
    }

    /// Forward a backspace keystroke to the PTY. Most shells (bash/zsh
    /// in default termios cooked mode) expect DEL (0x7F) for erase.
    @discardableResult
    public func sendBackspace() -> Bool {
        guard isPassthrough else { return false }
        send(Data([0x7F]))
        return true
    }
}
