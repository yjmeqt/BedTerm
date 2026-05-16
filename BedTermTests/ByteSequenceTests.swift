import Foundation
import Testing

@testable import BedTerm

@Suite("Bare-key byte sequences (R3)")
struct ByteSequenceTests {
    @Test(
        "bare keys from idle emit correct VT100 sequences",
        arguments: [
            (KeyTap.tab, Data([0x09])),
            (KeyTap.esc, Data([0x1B])),
            (KeyTap.up, Data([0x1B, 0x5B, 0x41])),
            (KeyTap.down, Data([0x1B, 0x5B, 0x42])),
            (KeyTap.right, Data([0x1B, 0x5B, 0x43])),
            (KeyTap.left, Data([0x1B, 0x5B, 0x44]))
        ])
    func bareKeysFromIdle(_ tap: KeyTap, _ expected: Data) {
        var state = KeyBarState.idle
        let outputs = state.reduce(tap, now: .anchor)
        #expect(outputs == [.bytes(expected)])
        #expect(state == .idle)
    }

    @Test(
        "bare keys from ctrlPending emit the bare sequence and exit pending (MVP: no Ctrl+arrow)",
        arguments: [
            KeyTap.tab, .esc, .up, .down, .left, .right
        ])
    func bareKeysFromPendingCancelAndSend(_ tap: KeyTap) {
        var state = KeyBarState.ctrlPending(startedAt: .anchor)
        let outputs = state.reduce(tap, now: .anchor)
        #expect(outputs.count == 2)
        #expect(outputs.last == .visualUnlatch)
        if case .bytes = outputs.first { /* ok */
        } else {
            Issue.record("expected first output to be bytes, got \(String(describing: outputs.first))")
        }
        #expect(state == .idle)
    }
}
