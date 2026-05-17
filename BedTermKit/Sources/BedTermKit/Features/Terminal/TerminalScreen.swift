import SwiftUI
import UIKit

struct TerminalScreen: View {
    @State var session: TerminalSession
    @State private var keyBar: KeyBarController
    @State private var composer: ComposerController
    @State private var bracketedPasteProbe: TerminalHostView.BracketedPasteProbe
    @State private var keyboardHidden: Bool = false
    @State private var keyboardOverlap: CGFloat = 0
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
        // Manage keyboard avoidance ourselves — SwiftUI's auto-avoidance
        // intermittently fails to release after resignFirstResponder, leaving
        // the bottom bar stuck mid-screen (bug r15_toolbar_stuck_midscreen).
        .ignoresSafeArea(.keyboard, edges: .bottom)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            bottomBar
                .padding(.bottom, keyboardOverlap)
        }
        .animation(.smooth(duration: 0.22), value: composer.isOpen)
        .onReceive(
            NotificationCenter.default.publisher(for: UIResponder.keyboardWillChangeFrameNotification)
        ) { note in
            updateKeyboardOverlap(from: note)
        }
        .onReceive(
            NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)
        ) { _ in
            keyboardOverlap = 0
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

    private func updateKeyboardOverlap(from note: Notification) {
        guard let userInfo = note.userInfo,
            let endFrame = userInfo[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect,
            let window = Self.keyWindow
        else { return }
        let windowFrame = window.bounds
        let intersection = windowFrame.intersection(endFrame)
        keyboardOverlap = max(0, intersection.height - window.safeAreaInsets.bottom)
    }

    private static var keyWindow: UIWindow? {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
    }

    private func reconnect() async {
        await session.connect(credential: credential, initialPTY: .init(cols: 80, rows: 24))
    }
}
