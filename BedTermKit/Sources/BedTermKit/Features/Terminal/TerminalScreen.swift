import SwiftUI

struct TerminalScreen: View {
    @State var session: TerminalSession
    @State private var keyBar: KeyBarController
    @State private var composer: ComposerController
    @State private var bracketedPasteProbe: TerminalHostView.BracketedPasteProbe
    @State private var keyboardHidden: Bool = false
    let credential: HostCredential
    let onExit: () -> Void

    init(session: TerminalSession, credential: HostCredential, onExit: @escaping () -> Void) {
        _session = State(initialValue: session)
        _keyBar = State(initialValue: KeyBarController { [weak session] data in session?.send(data) })
        let probe = TerminalHostView.BracketedPasteProbe()
        _bracketedPasteProbe = State(initialValue: probe)
        _composer = State(
            initialValue: ComposerController(
                send: { [weak session] data in session?.send(data) },
                isBracketedPasteActive: { MainActor.assumeIsolated { probe.isActive() } }
            ))
        self.credential = credential
        self.onExit = onExit
    }

    var body: some View {
        ZStack(alignment: .top) {
            TerminalHostView(
                feed: session.feed,
                onSend: { session.send($0) },
                onResize: { cols, rows in session.resize(cols: cols, rows: rows) },
                bracketedPasteProbe: bracketedPasteProbe,
                yieldFirstResponder: composer.isOpen || keyboardHidden
            )
            .ignoresSafeArea(edges: [.top, .horizontal])

            if case .closed(let reason) = session.state {
                DisconnectBanner(reason: reason) {
                    Task { await reconnect() }
                }
                .padding(.top, 8)
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if composer.isOpen {
                ComposerBar(controller: composer)
            } else {
                HStack(alignment: .center) {
                    KeyBar(
                        controller: keyBar,
                        keyboardShown: !keyboardHidden,
                        onToggleKeyboard: { keyboardHidden.toggle() }
                    )
                    Spacer()
                    ComposePill { composer.open() }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 6)
            }
        }
        .animation(.smooth(duration: 0.22), value: composer.isOpen)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Disconnect") {
                    session.disconnect()
                    onExit()
                }
            }
        }
        .navigationBarBackButtonHidden(true)
        .task {
            if case .idle = session.state {
                await session.connect(
                    credential: credential,
                    initialPTY: .init(cols: 80, rows: 24)
                )
            }
        }
    }

    private func reconnect() async {
        await session.connect(credential: credential, initialPTY: .init(cols: 80, rows: 24))
    }
}
