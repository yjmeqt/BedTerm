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

    @Test("submit without bracketed paste writes raw UTF-8 with CR line breaks and trailing CR; composer stays open and locks until the running command ends")
    func submitRaw() async {
        var captured: [Data] = []
        let controller = ComposerController(
            send: { captured.append($0) },
            isBracketedPasteActive: { false }
        )
        controller.open()
        controller.text = "echo hi\necho bye"
        controller.submit()
        #expect(captured == [Data("echo hi\recho bye\r".utf8)])
        // Submit keeps the just-sent command visible and locks the editor
        // (passthrough mode); endPassthrough() — fired when the running
        // block transitions to none-running — clears the buffer.
        #expect(controller.text == "echo hi\necho bye")
        #expect(controller.isPassthrough == true)
        #expect(controller.isOpen == true)
        controller.endPassthrough()
        #expect(controller.text == "")
        #expect(controller.isPassthrough == false)
    }

    @Test("submit appends a trailing CR so the last line auto-executes")
    func submitAutoExecutes() async {
        var captured: [Data] = []
        let controller = ComposerController(
            send: { captured.append($0) },
            isBracketedPasteActive: { false }
        )
        controller.text = "ls"
        controller.submit()
        #expect(captured == [Data("ls\r".utf8)])
    }

    @Test("submit with bracketed paste wraps payload and appends trailing CR after the close marker")
    func submitBracketed() async {
        var captured: [Data] = []
        let controller = ComposerController(
            send: { captured.append($0) },
            isBracketedPasteActive: { true }
        )
        controller.text = "line one\nline two"
        controller.submit()
        let prefix = Data([0x1B, 0x5B, 0x32, 0x30, 0x30, 0x7E])
        let suffix = Data([0x1B, 0x5B, 0x32, 0x30, 0x31, 0x7E])
        let body = Data("line one\nline two".utf8)
        let cr = Data([0x0D])
        let expected: Data = prefix + body + suffix + cr
        #expect(captured == [expected])
    }

    @Test("cancel discards buffer and closes composer")
    func cancelDiscards() async {
        var captured: [Data] = []
        let controller = ComposerController(
            send: { captured.append($0) },
            isBracketedPasteActive: { false }
        )
        controller.open()
        controller.text = "draft never sent"
        controller.cancel()
        #expect(controller.text == "")
        #expect(controller.isOpen == false)
        #expect(captured.isEmpty)
    }
}
