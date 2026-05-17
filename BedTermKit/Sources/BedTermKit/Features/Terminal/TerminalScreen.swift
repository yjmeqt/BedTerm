import SwiftUI

struct TerminalScreen: View {
    @State var session: TerminalSession
    @State private var keyBar: KeyBarController
    let credential: HostCredential
    let onExit: () -> Void

    init(session: TerminalSession, credential: HostCredential, onExit: @escaping () -> Void) {
        _session = State(initialValue: session)
        _keyBar = State(initialValue: KeyBarController { [weak session] data in session?.send(data) })
        self.credential = credential
        self.onExit = onExit
    }

    var body: some View {
        ZStack(alignment: .top) {
            TerminalHostView(
                feed: session.feed,
                onSend: { session.send($0) },
                onResize: { cols, rows in session.resize(cols: cols, rows: rows) },
                bracketedPasteProbe: TerminalHostView.BracketedPasteProbe()
            )
            .ignoresSafeArea(edges: [.top, .horizontal])

            if case .closed(let reason) = session.state {
                DisconnectBanner(reason: reason) {
                    Task { await reconnect() }
                }
                .padding(.top, 8)
            }
        }
        .safeAreaInset(edge: .bottom) {
            KeyBar(controller: keyBar)
        }
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
