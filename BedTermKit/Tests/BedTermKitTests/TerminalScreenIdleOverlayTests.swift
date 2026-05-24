import Testing

@testable import BedTermKit

@Suite("TerminalScreen connecting overlay predicate")
struct TerminalScreenIdleOverlayTests {
    @Test("Overlay covers .idle so the empty Metal grid is hidden before connect()")
    @MainActor
    func overlayShownWhileIdle() {
        #expect(TerminalSession.shouldShowConnectingOverlay(.idle))
    }

    @Test("Overlay covers .connecting while the SSH handshake is in flight")
    @MainActor
    func overlayShownWhileConnecting() {
        #expect(TerminalSession.shouldShowConnectingOverlay(.connecting))
    }

    @Test("Overlay hidden once the session is open")
    @MainActor
    func overlayHiddenWhenOpen() {
        #expect(!TerminalSession.shouldShowConnectingOverlay(.open))
    }

    @Test("Overlay hidden after the session closes (DisconnectBanner takes over)")
    @MainActor
    func overlayHiddenWhenClosed() {
        #expect(!TerminalSession.shouldShowConnectingOverlay(.closed(reason: "x")))
    }
}
