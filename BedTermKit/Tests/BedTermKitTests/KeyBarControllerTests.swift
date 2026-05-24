import Foundation
import Testing

@testable import BedTermKit

@Suite("KeyBarController")
@MainActor
struct KeyBarControllerTests {
    @Test("tapping CTRL then C emits 0x03")
    func ctrlThenC() async {
        var captured: [Data] = []
        let controller = KeyBarController { captured.append($0) }
        controller.handle(.ctrl)
        controller.handle(.char("c"))
        #expect(captured == [Data([0x03])])
    }

    @Test("tapping bare up emits ESC [ A")
    func bareUp() async {
        var captured: [Data] = []
        let controller = KeyBarController { captured.append($0) }
        controller.handle(.up)
        #expect(captured == [Data([0x1B, 0x5B, 0x41])])
    }

    @Test("isPending tracks ctrlPending state")
    func pendingFlag() async {
        let controller = KeyBarController { _ in }
        #expect(controller.isPending == false)
        controller.handle(.ctrl)
        #expect(controller.isPending == true)
        controller.handle(.ctrl)
        #expect(controller.isPending == false)
    }
}
