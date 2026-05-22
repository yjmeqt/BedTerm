import SwiftUI

struct TerminalScreen: View {
    @Environment(BedTermSettings.self) private var settings
    @State var session: TerminalSession
    @State private var keyBar: KeyBarController
    @State private var composer: ComposerController
    @State private var focusHandle: TerminalMetalHostView.FocusHandle
    @State private var keyboard = KeyboardLayoutObserver()
    @State private var keyboardHidden = false
    @State private var dpadOpen = false
    @Namespace private var composerMorph
    #if DEBUG
        @State private var fpsMeter = FPSMeter()
    #endif
    let credential: HostCredential
    /// Display name surfaced in the top bar centre title (PRD R3.title_shows_host_and_count).
    let hostName: String
    /// Leading Back chevron: pops without killing. `TerminalSession`
    /// stays alive on the view model so re-entry resumes the same
    /// shell (PRD R3.back_button).
    let onBack: () -> Void
    /// × Kill confirm: snapshots + tears down (PRD R3.kill_button).
    let onKill: (SessionSnapshot.KillReason) -> Void
    @State private var showingKillConfirm = false

    /// True when a full-screen TUI (vim, htop, claude) is on the remote side
    /// AND the user has the "Keep first row visible in full-screen apps"
    /// setting on. In that state the terminal viewport's top edge clamps to
    /// the top safe-area inset so the Dynamic Island / notch / status bar
    /// stops covering the TUI's first row (terminal-view
    /// R1.alt_screen_top_inset). The bottom toolbar (KeyBar + ComposePill)
    /// stays visible regardless — the user still needs Esc / Ctrl inside vim.
    private var reserveTopSafeArea: Bool {
        settings.reserveTopSafeAreaInAltScreen && session.mode.contains(.altScreen)
    }

    /// Safe-area edges the Metal terminal view should ignore. `.horizontal`
    /// always — the grid runs edge-to-edge. `.top` only when no TUI is in
    /// alt-screen; once a TUI takes over we surrender the inset back to the
    /// system so the Dynamic Island doesn't overlap meaningful content.
    private var ignoredTerminalEdges: Edge.Set {
        reserveTopSafeArea ? .horizontal : [.top, .horizontal]
    }

    /// Whether to show the Block list instead of the Classic grid. Block
    /// view falls back to Classic when a full-screen TUI is in alt-screen
    /// (terminal-view R3.alt_screen_collapse) — vim / htop / claude don't
    /// fit a per-command block model and need the live cursor + grid.
    private var showBlockView: Bool { displayMode == .blockList }

    /// Three-state display mode derived from settings + the live terminal
    /// mode flags. Drives both the upper visual stack (block list vs
    /// live grid) and the lower input stack (Warp-style flat composer
    /// vs the legacy ComposePill → ComposerBar morph).
    private var displayMode: TerminalDisplayMode {
        if session.mode.contains(.altScreen) { return .altScreen }
        return settings.showCommandBlocks ? .blockList : .inline
    }

    /// True while any block in the store is running. Drives the
    /// composer's passthrough lock release — when this flips back to
    /// false the just-finished command is done and the editor is free
    /// to accept the next one.
    private var hasRunningBlock: Bool {
        session.blockStore.blocks.contains(where: \.isRunning)
    }

    #if DEBUG
        /// Floating debug chip: live PTY geometry + display mode + FPS.
        /// `cols=32` means claude is starved; `mode=alt` lingering after
        /// a TUI exits means we leaked the bit; FPS spots renderer stalls.
        private var geomHUD: some View {
            VStack(alignment: .trailing, spacing: 2) {
                debugChip(text: "\(session.terminalCore.screenCols)×\(session.terminalCore.screenRows)")
                debugChip(text: debugModeIndicator ?? "")
                debugChip(text: "\(fpsMeter.fps) fps")
            }
        }

        private func debugChip(text: String) -> some View {
            Text(verbatim: text)
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                .padding(.horizontal, 6)
                .padding(.vertical, 3)
                .background(
                    RoundedRectangle(cornerRadius: 4, style: .continuous)
                        .fill(
                            Color("ShadcnMutedForeground", bundle: .module)
                                .opacity(0.15))
                )
        }
    #endif

    /// Snapshot of the context chips shown above the block-list
    /// composer's input. Reads the unfiltered Rust block list so the
    /// pending block's `pwd` (open but not yet command-filled) still
    /// lights up the cwd chip; the Swift `BlockStore` mirror hides
    /// that pending entry by design.
    private var promptContext: PromptContext {
        // Read cwd / branch from BlockStore (observable, so the body
        // re-renders on every Precmd). The last sealed exit code comes
        // from the visible block list. `host` is constant per session.
        let lastSealedExit = session.blockStore.blocks
            .reversed().lazy
            .first(where: { !$0.isRunning })?.exitCode
        return PromptContext(
            host: credential.host,
            cwd: session.blockStore.latestPwd,
            lastExitCode: lastSealedExit,
            gitBranch: session.blockStore.latestGitBranch
        )
    }

    init(
        session: TerminalSession,
        credential: HostCredential,
        hostName: String = "",
        onBack: @escaping () -> Void,
        onKill: @escaping (SessionSnapshot.KillReason) -> Void
    ) {
        _session = State(initialValue: session)
        _keyBar = State(initialValue: KeyBarController { [weak session] data in session?.send(data) })
        let focusHandle = TerminalMetalHostView.FocusHandle()
        _focusHandle = State(initialValue: focusHandle)
        _composer = State(
            initialValue: ComposerController(
                send: { [weak session] data in session?.send(data) },
                isBracketedPasteActive: { [weak session] in
                    session?.mode.contains(.bracketedPaste) ?? false
                },
                returnFocusToTerminal: {
                    MainActor.assumeIsolated { focusHandle.claimFirstResponder() }
                }
            ))
        self.credential = credential
        self.hostName =
            hostName.isEmpty
            ? "\(credential.username)@\(credential.host)"
            : hostName
        self.onBack = onBack
        self.onKill = onKill
    }

    var body: some View {
        VStack(spacing: 0) {
            ZStack(alignment: .top) {
                // Always mount the Metal host view — it owns the
                // TerminalCore that the BlockStore reads from, and that
                // store needs to keep building blocks even while the user
                // is looking at the Block list. We just hide it when the
                // Block list is visible so the GPU isn't redundantly
                // presenting both.
                TerminalMetalHostView(
                    session: session,
                    feed: session.feed,
                    onSend: { session.send($0) },
                    onResize: { cols, rows in session.resize(cols: cols, rows: rows) },
                    focusHandle: focusHandle,
                    yieldFirstResponder: composer.isOpen || keyboardHidden
                )
                .ignoresSafeArea(edges: ignoredTerminalEdges)
                .opacity(showBlockView ? 0 : 1)
                // Top-inset toggle must be instant (terminal-view
                // R1.alt_screen_top_inset). Animating a mid-frame PTY reflow
                // tears vim/htop's UI; suppress any animation that the
                // surrounding view tree might otherwise carry into the
                // safe-area change.
                .transaction(value: reserveTopSafeArea) { $0.animation = nil }

                if showBlockView {
                    BlockListView(session: session)
                        .transition(.opacity)
                }

                if case .closed(let reason) = session.state {
                    DisconnectBanner(reason: reason) {
                        Task { await reconnect() }
                    }
                    .padding(.top, 8)
                }

                #if DEBUG
                    geomHUD
                        .padding(.top, 4)
                        .padding(.trailing, 8)
                        .frame(
                            maxWidth: .infinity, maxHeight: .infinity,
                            alignment: .topTrailing
                        )
                        .allowsHitTesting(false)
                #endif

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
        .animation(.smooth(duration: 0.22), value: showBlockView)
        .onChange(of: composer.isOpen) { _, isOpen in
            if isOpen { dpadOpen = false }
        }
        // Mirror Warp's "command done → editor unlocks" handoff: when
        // the running block transitions to none-running, drop the
        // passthrough lock so the composer accepts the next command's
        // text instead of streaming it as stdin to the (now-finished)
        // process. Watching the boolean (not the array) keeps this
        // .onChange fire when running state actually flips, ignoring
        // intra-running snapshot updates.
        .onChange(of: hasRunningBlock) { _, isRunning in
            if !isRunning { composer.endPassthrough() }
        }
        .toolbar {
            // Back + × cluster on the leading side; trailing reserved
            // for a future per-session action (PRD R3).
            ToolbarItemGroup(placement: .topBarLeading) {
                Button {
                    onBack()
                } label: {
                    Image(systemName: "chevron.left")
                }
                .accessibilityLabel(Text("Back"))
                .accessibilityIdentifier("terminal.back")
                Button(role: .destructive) {
                    showingKillConfirm = true
                } label: {
                    Image(systemName: "xmark")
                        .foregroundStyle(Color("ShadcnDestructive", bundle: .module))
                }
                .accessibilityLabel(Text("Kill session"))
                .accessibilityIdentifier("terminal.kill")
            }
            ToolbarItem(placement: .principal) {
                Text(verbatim: hostName)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .confirmationDialog(
            Text("End this session?"),
            isPresented: $showingKillConfirm,
            titleVisibility: .visible
        ) {
            Button(String(localized: "End session"), role: .destructive) {
                onKill(.userKilled)
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        }
        .navigationBarBackButtonHidden(true)
        .task {
            if case .idle = session.state {
                await session.connect(
                    credential: credential,
                    initialPTY: .init(cols: 80, rows: 24),
                    bootstrapPayload: bootstrapPayload()
                )
            }
        }
    }

    /// Resolve the shell-integration payload to push at connect time. Returns
    /// `nil` when the user hasn't opted in, so the channel stays pristine.
    private func bootstrapPayload() -> String? {
        guard settings.installShellIntegrationOnConnect else { return nil }
        return ShellIntegrationScript.bootstrapPayload()
    }

    @ViewBuilder
    private var bottomBar: some View {
        // Single component for every mode — `.blockList` keeps cwd +
        // input always visible; `.launcher` (inline / alt-screen)
        // starts collapsed, exposes the ✎ Compose chip on the right,
        // and closes itself on submit so raw PTY input resumes.
        BlockListComposer(
            controller: composer, keyBar: keyBar,
            mode: displayMode == .blockList ? .blockList : .launcher,
            context: promptContext,
            keyboardShown: !keyboardHidden,
            dpadOpen: dpadOpen,
            onToggleKeyboard: { keyboardHidden.toggle() },
            onToggleDpad: { dpadOpen.toggle() }
        )
        .onAppear {
            if displayMode == .blockList && !composer.isOpen {
                composer.open()
            } else if displayMode != .blockList && composer.isOpen {
                composer.cancel()
            }
        }
        .onChange(of: displayMode) { _, newMode in
            if newMode == .blockList && !composer.isOpen {
                composer.open()
            } else if newMode != .blockList && composer.isOpen {
                // Hard close — even with a pending draft. The mode
                // transition (e.g. entering alt-screen) means raw PTY
                // should resume immediately.
                composer.cancel()
            }
        }
    }

    /// DEBUG-only chip text that displays the resolved displayMode plus
    /// the raw terminal mode flags. Lets us verify alt-screen detection
    /// from inside the running app without a debugger attached.
    private var debugModeIndicator: String? {
        #if DEBUG
            var flags: [String] = []
            if session.mode.contains(.altScreen) { flags.append("alt") }
            if session.mode.contains(.bracketedPaste) { flags.append("bp") }
            let modeName: String
            switch displayMode {
            case .blockList: modeName = "block"
            case .inline: modeName = "inline"
            case .altScreen: modeName = "alt"
            }
            return flags.isEmpty
                ? "mode=\(modeName)"
                : "mode=\(modeName) [\(flags.joined(separator: ","))]"
        #else
            return nil
        #endif
    }

    private func reconnect() async {
        await session.connect(
            credential: credential,
            initialPTY: .init(cols: 80, rows: 24),
            bootstrapPayload: bootstrapPayload()
        )
    }
}

#if DEBUG
    extension TerminalScreen {
        /// Debug-only: build a TerminalScreen against a caller-supplied SSH
        /// client (e.g. CitadelSSHClient pointed at the loopback
        /// `bedterm-mock-ssh` server). Bypasses the hosts list / view model.
        init(
            mockSSHClient: any SSHClient,
            credential: HostCredential,
            onExit: @escaping () -> Void
        ) {
            let session = TerminalSession(client: mockSSHClient, hostID: UUID(), persistence: nil)
            self.init(
                session: session,
                credential: credential,
                hostName: "mock-ssh",
                onBack: onExit,
                onKill: { _ in onExit() }
            )
        }
    }
#endif
