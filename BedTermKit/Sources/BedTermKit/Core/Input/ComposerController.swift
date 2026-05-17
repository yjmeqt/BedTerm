import Foundation
import Observation

@MainActor
@Observable
public final class ComposerController {
    public var text: String = ""
    public private(set) var isOpen: Bool = false

    private let send: (Data) -> Void
    private let isBracketedPasteActive: () -> Bool

    public init(
        send: @escaping (Data) -> Void,
        isBracketedPasteActive: @escaping () -> Bool
    ) {
        self.send = send
        self.isBracketedPasteActive = isBracketedPasteActive
    }

    public func open() {
        isOpen = true
    }

    public func cancel() {
        text = ""
        isOpen = false
    }

    public func submit() {
        guard !text.isEmpty else { return }
        let trailingReturn = Data([0x0D]) // CR — runs the last line
        let bytes: Data
        if isBracketedPasteActive() {
            let prefix = Data([0x1B, 0x5B, 0x32, 0x30, 0x30, 0x7E]) // ESC [ 200~
            let suffix = Data([0x1B, 0x5B, 0x32, 0x30, 0x31, 0x7E]) // ESC [ 201~
            bytes = prefix + Data(text.utf8) + suffix + trailingReturn
        } else {
            bytes = Data(text.replacingOccurrences(of: "\n", with: "\r").utf8) + trailingReturn
        }
        send(bytes)
        text = ""
        isOpen = false
    }
}
