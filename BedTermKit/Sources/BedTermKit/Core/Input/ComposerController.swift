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
        text = ""
        isOpen = false
    }
}
