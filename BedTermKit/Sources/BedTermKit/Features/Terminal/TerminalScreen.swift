import SwiftUI

struct TerminalScreen: View {
    @State var session: TerminalSession
    @State private var keyBar: KeyBarController
    @State private var composer: ComposerController
    @State private var bracketedPasteProbe: TerminalHostView.BracketedPasteProbe
    @State private var keyboard = KeyboardLayoutObserver()
    @State private var keyboardHidden = false
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
        VStack(spacing: 0) {
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
            .frame(maxHeight: .infinity)

            bottomBar
        }
        // Manage keyboard avoidance ourselves: pad by the observed keyboard
        // overlap, then ignore SwiftUI's auto-applied keyboard safe area on
        // the resulting padded view. Modifier order matters — applying
        // ignoresSafeArea inside the padding causes the outer view to still
        // respect SwiftUI's keyboard inset, double-counting the keyboard
        // height and stranding the bar mid-screen.
        .padding(.bottom, keyboard.overlap)
        .ignoresSafeArea(.keyboard, edges: .bottom)
        .animation(.smooth(duration: 0.22), value: composer.isOpen)
        .animation(.smooth(duration: 0.22), value: keyboard.overlap)
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

    @ViewBuilder
    private var bottomBar: some View {
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

    private func reconnect() async {
        await session.connect(credential: credential, initialPTY: .init(cols: 80, rows: 24))
    }
}
