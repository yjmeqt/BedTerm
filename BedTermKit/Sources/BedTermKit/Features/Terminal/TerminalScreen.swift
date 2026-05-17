import SwiftUI

struct TerminalScreen: View {
    @State var session: TerminalSession
    @State private var keyBar: KeyBarController
    @State private var composer: ComposerController
    @State private var bracketedPasteProbe: TerminalHostView.BracketedPasteProbe
    @State private var focusHandle: TerminalHostView.FocusHandle
    @State private var keyboard = KeyboardLayoutObserver()
    @State private var keyboardHidden = false
    @State private var dpadOpen = false
    @Namespace private var composerMorph
    #if DEBUG
        @AppStorage("debug.useMetalRenderer") private var useMetalRenderer: Bool = false
    #endif
    let credential: HostCredential
    let onExit: () -> Void

    init(session: TerminalSession, credential: HostCredential, onExit: @escaping () -> Void) {
        _session = State(initialValue: session)
        _keyBar = State(initialValue: KeyBarController { [weak session] data in session?.send(data) })
        let probe = TerminalHostView.BracketedPasteProbe()
        _bracketedPasteProbe = State(initialValue: probe)
        let focusHandle = TerminalHostView.FocusHandle()
        _focusHandle = State(initialValue: focusHandle)
        _composer = State(
            initialValue: ComposerController(
                send: { [weak session] data in session?.send(data) },
                isBracketedPasteActive: { MainActor.assumeIsolated { probe.isActive() } },
                returnFocusToTerminal: {
                    MainActor.assumeIsolated { focusHandle.claimFirstResponder() }
                }
            ))
        self.credential = credential
        self.onExit = onExit
    }

    var body: some View {
        VStack(spacing: 0) {
            ZStack(alignment: .top) {
                #if DEBUG
                    if useMetalRenderer {
                        TerminalMetalHostView(
                            feed: session.feed,
                            onSend: { session.send($0) },
                            onResize: { cols, rows in session.resize(cols: cols, rows: rows) },
                            yieldFirstResponder: composer.isOpen || keyboardHidden
                        )
                        .ignoresSafeArea(edges: [.top, .horizontal])
                    } else {
                        TerminalHostView(
                            feed: session.feed,
                            onSend: { session.send($0) },
                            onResize: { cols, rows in session.resize(cols: cols, rows: rows) },
                            bracketedPasteProbe: bracketedPasteProbe,
                            focusHandle: focusHandle,
                            yieldFirstResponder: composer.isOpen || keyboardHidden
                        )
                        .ignoresSafeArea(edges: [.top, .horizontal])
                    }
                #else
                    TerminalHostView(
                        feed: session.feed,
                        onSend: { session.send($0) },
                        onResize: { cols, rows in session.resize(cols: cols, rows: rows) },
                        bracketedPasteProbe: bracketedPasteProbe,
                        focusHandle: focusHandle,
                        yieldFirstResponder: composer.isOpen || keyboardHidden
                    )
                    .ignoresSafeArea(edges: [.top, .horizontal])
                #endif

                if case .closed(let reason) = session.state {
                    DisconnectBanner(reason: reason) {
                        Task { await reconnect() }
                    }
                    .padding(.top, 8)
                }

                if dpadOpen {
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture { dpadOpen = false }
                        .accessibilityIdentifier("dpad.scrim")
                        .accessibilityLabel("Hide direction pad")
                        .transition(.opacity)

                    VStack {
                        Spacer(minLength: 0)
                        DirectionPad(
                            onDirection: { tap in keyBar.handle(tap) },
                            onClose: { dpadOpen = false }
                        )
                        .padding(.bottom, 12)
                    }
                    .transition(.scale(scale: 0.85, anchor: .bottom).combined(with: .opacity))
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
        // `keyboard.overlap` is already animated inside KeyboardLayoutObserver using
        // the system keyboard's own duration — do NOT layer another .animation on it.
        .animation(.smooth(duration: 0.22), value: composer.isOpen)
        .animation(.smooth(duration: 0.22), value: dpadOpen)
        .onChange(of: composer.isOpen) { _, isOpen in
            if isOpen { dpadOpen = false }
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
        // Wrap KeyBar + ComposePill + ComposerBar in a single GlassEffectContainer
        // so the Liquid Glass capsule with id "composer-capsule" morphs
        // continuously from the closed-state pill into the open-state composer
        // bar (R13.open_close_morph). The KeyBar slides off the leading edge to
        // make room for the morph (R13.row_height_symmetry).
        GlassEffectContainer(spacing: 8) {
            HStack(alignment: .bottom, spacing: 8) {
                if !composer.isOpen {
                    KeyBar(
                        controller: keyBar,
                        keyboardShown: !keyboardHidden,
                        dpadOpen: dpadOpen,
                        onToggleKeyboard: { keyboardHidden.toggle() },
                        onToggleDpad: { dpadOpen.toggle() }
                    )
                    .transition(.move(edge: .leading).combined(with: .opacity))
                    Spacer(minLength: 0)
                    ComposePill(morphNamespace: composerMorph) { composer.open() }
                } else {
                    ComposerBar(controller: composer, morphNamespace: composerMorph)
                        .frame(maxWidth: .infinity)
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 6)
        }
        .animation(.smooth(duration: 0.32), value: composer.isOpen)
    }

    private func reconnect() async {
        await session.connect(credential: credential, initialPTY: .init(cols: 80, rows: 24))
    }
}
