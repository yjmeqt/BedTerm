import Foundation
import Testing

@testable import BedTermKit

@Suite("ComposerController")
@MainActor
struct ComposerControllerTests {
    @Test("submit with empty buffer is a no-op")
    func submitEmpty() async {
        var captured: [Data] = []
        let controller = ComposerController(
            send: { captured.append($0) },
            isBracketedPasteActive: { false }
        )
        controller.open()
        controller.submit()
        #expect(captured.isEmpty)
        #expect(controller.isOpen == true)
    }
}
