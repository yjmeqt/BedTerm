import SwiftUI
import UIKit

/// Flat, always-open composer rendered as the trailing entry in the
/// block list "stream". Matches block-card ASCII rhythm — same mono
/// font, same left inset, same hairline divider above. No card chrome
/// (no rounded panel, no shadow), shadcn-flavoured restraint:
///
///   ──────────────────────────────────────── ← hairline (Swift divider)
///   📁 ~/dev/bedterm                          ← prompt strip (cwd chip)
///   ❯ git status --short_                     ← input  (mono 14)
///       newline   ⌃ Ctrl                  ▶ Run
///
/// Enter submits. Multi-line via the explicit "newline" button (or by
/// pasting a string that already contains `\n`). The Run button is the
/// primary action; the rest of the footer row is a quiet affordances
/// strip.
/// How the composer wears in a given display mode.
///   - `.blockList` : cwd + input always visible, run keeps composer
///     open so the user can immediately dispatch another command.
///   - `.launcher`  : cwd hidden; input hidden until the right-side
///     ✎ chip is tapped; submitting closes the composer again so
///     the raw PTY can resume taking bytes from the system keyboard.
enum ComposerMode {
    case blockList
    case launcher
}

struct BlockListComposer: View {
    @Bindable var controller: ComposerController
    @Bindable var keyBar: KeyBarController
    var mode: ComposerMode = .blockList
    var context: PromptContext = .init()
    var keyboardShown: Bool = true
    var dpadOpen: Bool = false
    /// DEBUG only — pass `true` to overlay a tiny chip showing the
    /// resolved `displayMode` + alt-screen flag in the cwd row. Helps
    /// diagnose mode-detection bugs (e.g. "claude entered alt-screen
    /// but blocks are still showing"). Should be off in release.
    var debugModeChip: String?
    var onToggleKeyboard: () -> Void = {}
    var onToggleDpad: () -> Void = {}
    @State private var contentHeight: CGFloat = 0
    @State private var isFocused: Bool = false

    private var showsCwd: Bool { mode == .blockList }
    private var showsInput: Bool {
        switch mode {
        case .blockList: return true
        case .launcher: return controller.isOpen
        }
    }
    private var showsRun: Bool {
        switch mode {
        case .blockList: return true
        case .launcher: return controller.isOpen
        }
    }
    private var showsComposerLauncher: Bool {
        mode == .launcher && !controller.isOpen
    }

    private let horizontalInset: CGFloat = 16
    private let verticalPadding: CGFloat = 10
    private let chevronColumn: CGFloat = 18
    private let singleLineHeight: CGFloat = 22
    private let maxBodyHeight: CGFloat = 5 * 22 + 8

    private var bodyHeight: CGFloat {
        min(max(singleLineHeight, contentHeight), maxBodyHeight)
    }

    var body: some View {
        VStack(spacing: 0) {
            divider
            if showsCwd {
                promptStrip
                    .padding(.horizontal, horizontalInset)
                    .padding(.top, verticalPadding)
                    .padding(.bottom, 4)
            }
            if showsInput {
                inputRow
                    .padding(.horizontal, horizontalInset)
                    .padding(.top, showsCwd ? 0 : verticalPadding)
                    .padding(.bottom, 6)
            }
            footer
                .padding(.horizontal, horizontalInset)
                .padding(.top, showsInput ? 0 : verticalPadding)
                .padding(.bottom, verticalPadding)
        }
        .background(
            // Subtle focus halo — instead of a hard border, lift the
            // background a hair when the user is composing. Reads as
            // "this is the active row" without breaking the flat list.
            Color("Accent", bundle: .module)
                .opacity(isFocused ? 0.04 : 0)
                .animation(.smooth(duration: 0.18), value: isFocused)
        )
        .animation(.smooth(duration: 0.18), value: showsInput)
    }

    private var divider: some View {
        Rectangle()
            .fill(Color("ShadcnBorder", bundle: .module).opacity(0.5))
            .frame(height: BlockPanelStyle.dividerThicknessPt)
            .padding(.horizontal, BlockPanelStyle.dividerHorizontalInsetPt)
    }

    @ViewBuilder
    private var promptStrip: some View {
        // v1 visible: cwd only. host / git_branch / last-exit are wired
        // through `PromptContext` (so the data is already flowing) but
        // intentionally hidden until we decide the layout — drop the
        // outer `if` and uncomment the additional chips to enable them.
        if let cwd = displayCwd {
            HStack(spacing: 6) {
                chip(icon: "folder", text: cwd)
                Spacer(minLength: 0)
                // if let host = context.host {
                //     chip(icon: "server.rack", text: host)
                // }
                // if let branch = context.gitBranch, !branch.isEmpty {
                //     chip(icon: "arrow.triangle.branch", text: branch)
                // }
                // if let exit = context.lastExitCode, exit != 0 {
                //     chip(
                //         icon: "xmark.circle.fill",
                //         text: "exit \(exit)",
                //         accent: Color("ShadcnDestructive", bundle: .module)
                //     )
                // }
            }
            .lineLimit(1)
            .truncationMode(.head)
        }
    }

    private var displayCwd: String? {
        guard let raw = context.cwd, !raw.isEmpty else { return nil }
        let maxChars = 28
        if raw.count <= maxChars { return raw }
        let suffix = raw.suffix(maxChars - 1)
        return "…\(suffix)"
    }

    private func chip(icon: String, text: String, accent: Color? = nil) -> some View {
        HStack(spacing: 4) {
            Image(systemName: icon)
                .font(.system(size: 10, weight: .medium))
            Text(verbatim: text)
                .font(.system(size: 11, design: .monospaced))
        }
        .foregroundStyle(accent ?? Color("ShadcnMutedForeground", bundle: .module))
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .background(
            RoundedRectangle(cornerRadius: 5, style: .continuous)
                .fill(Color("ShadcnMutedForeground", bundle: .module).opacity(0.08))
        )
    }

    private var inputRow: some View {
        HStack(alignment: .top, spacing: 0) {
            Text(verbatim: "❯")
                .font(.system(.body, design: .monospaced).weight(.semibold))
                .foregroundStyle(Color.accentColor)
                .frame(width: chevronColumn, alignment: .leading)
                .padding(.top, 2)
            ComposerTextView(
                text: $controller.text,
                contentHeight: $contentHeight,
                placeholder: String(localized: "Type a command — Enter to run"),
                isFocused: isFocused,
                onFocusChange: { isFocused = $0 },
                onSubmit: { handleSubmit() }
            )
            .frame(height: bodyHeight)
        }
    }

    private var footer: some View {
        HStack(spacing: 0) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 0) {
                    if let debug = debugModeChip {
                        Text(verbatim: debug)
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 4)
                            .background(
                                RoundedRectangle(cornerRadius: 4, style: .continuous)
                                    .fill(Color("ShadcnMutedForeground", bundle: .module)
                                        .opacity(0.1))
                            )
                            .padding(.trailing, 4)
                    }
                    keyChip(
                        .tab, icon: "arrow.right.to.line.compact",
                        label: String(localized: "Tab"), id: "tab")
                    footerChip(icon: "return", label: String(localized: "Newline")) {
                        controller.text += "\n"
                    }
                    .accessibilityIdentifier("composer.newline")
                    keyChip(
                        .esc, icon: "escape",
                        label: String(localized: "Esc"), id: "esc")
                    keyChip(
                        .ctrl, icon: "control",
                        label: String(localized: "Ctrl"),
                        id: "ctrl", highlighted: keyBar.isPending)
                    footerChip(
                        icon: "clock.arrow.circlepath",
                        label: String(localized: "History")
                    ) {
                        // History popover wired in next iteration.
                    }
                    .accessibilityIdentifier("composer.history")
                    .disabled(true)
                    .opacity(0.5)
                }
                .padding(.horizontal, 4)
                .fixedSize(horizontal: true, vertical: false)
            }

            // Pinned right cluster: divider + dpad + kbd toggle + Run.
            // Always visible, never scrolls, never shrinks.
            chipDivider
            chromeChip(
                icon: dpadOpen ? "dpad.fill" : "dpad",
                accent: dpadOpen,
                action: onToggleDpad
            )
            .accessibilityIdentifier("composer.dpad")
            .accessibilityLabel(dpadOpen ? "Hide direction pad" : "Show direction pad")

            chromeChip(
                icon: "keyboard.chevron.compact.down",
                accent: false,
                flipped: !keyboardShown,
                action: onToggleKeyboard
            )
            .accessibilityIdentifier("composer.kbtoggle")
            .accessibilityLabel(keyboardShown ? "Hide keyboard" : "Show keyboard")

            if showsRun {
                runButton
                    .padding(.leading, 8)
            }
            if showsComposerLauncher {
                composerLauncherChip
                    .padding(.leading, 4)
            }
        }
        .font(.system(size: 12))
    }

    /// `.launcher` mode's "open the multi-line editor" chip — replaces
    /// the run button when the input row is hidden. Tapping it flips
    /// `controller.isOpen` to true so the input row + run button slide
    /// in (same component, same row layout).
    private var composerLauncherChip: some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            controller.open()
        } label: {
            HStack(spacing: 4) {
                Image(systemName: "square.and.pencil")
                    .font(.system(size: 11, weight: .medium))
                Text(verbatim: String(localized: "Compose"))
                    .font(.system(size: 12))
            }
            .foregroundStyle(Color.accentColor)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("composer.launcher")
    }

    /// Submit the draft and — for `.launcher` mode — close the composer
    /// so the raw PTY resumes receiving keyboard bytes directly. Used
    /// by both the Run button tap and the ComposerTextView's Enter
    /// keypath so the two paths stay in sync.
    private func handleSubmit() {
        controller.submit()
        if mode == .launcher { controller.cancel() }
    }

    private var runButton: some View {
        Button {
            UINotificationFeedbackGenerator().notificationOccurred(.success)
            handleSubmit()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "return")
                    .font(.system(size: 12, weight: .bold))
                Text(verbatim: "Run")
                    .font(.system(size: 13, weight: .semibold))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .foregroundStyle(Color("ShadcnPrimaryForeground", bundle: .module))
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(Color("ShadcnPrimary", bundle: .module))
            )
            .opacity(controller.text.isEmpty ? 0.4 : 1)
        }
        .buttonStyle(.plain)
        .disabled(controller.text.isEmpty)
        .accessibilityIdentifier("composer.run")
        .accessibilityLabel("Run command")
    }

    private var chipDivider: some View {
        Rectangle()
            .fill(Color("ShadcnBorder", bundle: .module).opacity(0.6))
            .frame(width: 1, height: 18)
            .padding(.horizontal, 6)
    }

    private func chromeChip(
        icon: String, accent: Bool, flipped: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.system(size: 12, weight: .medium))
                .scaleEffect(y: flipped ? -1 : 1)
                .foregroundStyle(
                    accent
                        ? Color.accentColor
                        : Color("ShadcnMutedForeground", bundle: .module))
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func footerChip(
        icon: String,
        label: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.system(size: 11, weight: .medium))
                Text(verbatim: label)
                    .font(.system(size: 12))
            }
            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// KeyBarController-backed chip for the raw-PTY modifier keys
    /// (Esc / Ctrl / Tab) now living in this footer row instead of in
    /// the separate KeyBar below. `highlighted` swaps the chip text +
    /// icon to `accentColor` (used by the Ctrl latch state).
    private func keyChip(
        _ tap: KeyTap, icon: String, label: String, id: String,
        highlighted: Bool = false
    ) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: highlighted ? .medium : .light)
                .impactOccurred()
            keyBar.handle(tap)
        } label: {
            HStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.system(size: 11, weight: .medium))
                Text(verbatim: label)
                    .font(.system(size: 12))
            }
            .foregroundStyle(
                highlighted
                    ? Color.accentColor
                    : Color("ShadcnMutedForeground", bundle: .module))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("composer.\(id)")
    }
}
