# BedTerm MVP — iOS Implementation Spec

PRD: `prd/bedterm/mvp.xml`. This document describes **how** the MVP is built on iOS. Behavior lives in the PRD.

## Architecture

SwiftUI + UIKit interop. Three concentric layers:

- **App** (`bedterm/App/`) — `BedTermApp` root, `AppRoute` enum for typed navigation (`.terminal`, `.hostKeyMismatch`).
- **Features** (`bedterm/Features/`) — screen-level views and view models. Each feature owns its own state.
  - `Connection/` — `ConnectionScreen`, `ConnectionViewModel`, `HostKeyMismatchScreen`.
  - `Terminal/` — `TerminalScreen`, `TerminalSession` (`@Observable`), `TerminalHostView` (UIKit bridge for SwiftTerm), `KeyBar`, `KeyBarController`, `DisconnectBanner`.
- **Core** (`bedterm/Core/`) — platform-agnostic primitives.
  - `SSH/` — `SSHClient` protocol; `CitadelSSHClient` production impl over Citadel; `MockSSHClient` for tests; `SSHErrorMapping` translates Citadel/NIO errors into `SSHError` cases.
  - `Keychain/` — `Keychain` wrapper; `CredentialsStore` (single host record); `HostKeyStore` (host:port → SHA-256 fingerprint).
  - `Input/` — `KeyBarState` reducer (idle / ctrl-pending) + `KeyTap` events + `KeyBarOutput` effects.

## Key types

| Type | Role |
|---|---|
| `SSHClient` (protocol) | `connect`, `write`, `resize`, `disconnect`, `output: AsyncStream<Data>` — abstracts Citadel for testability. |
| `TerminalSession` | `@Observable` state machine (`idle / connecting / open / closed`); owns the feed AsyncStream from `SSHClient.output` and forwards user input. |
| `KeyBarState` | Pure reducer. `reduce(KeyTap, now:) -> [KeyBarOutput]`. Inputs: `.ctrl / .char / .tab / .esc / arrows / .tick`. Outputs: `.bytes(Data) / .visualLatch / .visualUnlatch / .noop`. |
| `KeyBarController` | `@Observable` host for `KeyBarState`. Owns the tick task that fires `.tick` every ~200 ms while pending; cancels on idle. |
| `HostKeyStore` | Keychain-backed `host:port → SHA256 fingerprint`. Powers TOFU compare in `CitadelSSHClient`. |

## Terminal rendering

SwiftTerm's `TerminalView` (UIKit) is wrapped by `TerminalHostView: UIViewRepresentable`. SwiftTerm handles ANSI/VT100 parsing, color, cursor, scrollback, text selection, copy/paste menu, monospace font, and resize. The host view's `Coordinator`:
- Consumes `feed: AsyncStream<Data>` and pumps bytes via `view.feed(byteArray:)`.
- Forwards `TerminalViewDelegate.send(...)` user input to `TerminalSession.send`.
- Forwards `sizeChanged(...)` to `TerminalSession.resize` → `SSHClient.resize`.
- Implements `clipboardCopy` → `UIPasteboard.general.string`.

## Ctrl-pending state machine

`KeyBarState` is a discriminated union driven by `KeyTap` events:

1. `idle + .ctrl` → `ctrlPending(now)`, emit `.visualLatch`.
2. `ctrlPending + .char(c)` → `idle`, emit `bytes([c.lowercased() & 0x1F])` + `.visualUnlatch`. Implements Ctrl+A..Z generically — Ctrl+C/D/Z/L/A/E require no special casing.
3. `ctrlPending + .ctrl` → `idle`, `.visualUnlatch` (toggle cancel).
4. `ctrlPending + .tick(now)` → if `now - startedAt > 3s`, return to `idle` with `.visualUnlatch`.
5. Any state + `.tab / .esc / arrow` → unlatch if pending, emit byte sequence (`0x09` / `0x1B` / `ESC [ A|B|C|D`).

## SSH + TOFU flow

1. `ConnectionViewModel.connect()` builds a `HostCredential` from the form and calls `SSHClient.connect`.
2. `CitadelSSHClient` opens TCP → SSH handshake → captures remote host key fingerprint (`SHA256(base64)`).
3. Compares against `HostKeyStore[host:port]`:
   - missing → store, proceed (TOFU).
   - match → proceed silently.
   - mismatch → throw `SSHError.hostKeyMismatch(stored, remote)`. View model captures `pendingMismatch` and routes to `HostKeyMismatchScreen`. On Trust, re-call `connect()` after store update.
4. Authenticate (`.password` or `.privateKey` w/ optional passphrase). Errors map to `SSHError` and surface as `viewModel.errorMessage`.
5. On success: open PTY (default 80×24), expose `output: AsyncStream<Data>` for `TerminalSession`.

## Navigation

`NavigationStack(path: $path)` rooted at `ConnectionScreen`. `AppRoute` cases:
- `.terminal` — pushes `TerminalScreen` with the live `TerminalSession` and captured credential.
- `.hostKeyMismatch(stored, remote, host, port)` — pushes the TOFU warning screen.

Disconnect resets `path` to empty (back to ConnectionScreen).

## Appearance & orientation

- The app follows the iOS system appearance (R9). Remove the root `.preferredColorScheme(.dark)` so SwiftUI inherits `UITraitCollection.current.userInterfaceStyle`.
- **All app-chrome colours come from the asset catalog.** Token catalogue and Light/Dark values live in the [Design PRD R1 — Colour Tokens](../../../prd/bedterm/design.xml). The catalog itself lives in `BedTermKit/Sources/BedTermKit/Resources/Tokens.xcassets/`; each `*.colorset` has an `Any Appearance` and `Dark Appearance` variant. Reference tokens via the typed `Color` extension in `BedTermKit/Sources/BedTermKit/DesignSystem/Color+Tokens.swift` (`Color.surfaceBackground`, `Color.accentLabel`, etc.).
  - Adaptive SwiftUI symbolic colours (`Color.primary`, `Color.secondary`, `Color.accentColor`, `.tint`, `.regularMaterial`) are acceptable where no brand-specific token applies — they already resolve per appearance.
- **Terminal palette deferred (T13b).** SwiftTerm's `TerminalTheme` bridge and `traitCollectionDidChange` rebuild remain ❌; for now the terminal keeps its existing palette. Rules `terminal_palette_light` / `terminal_palette_dark` stay open.
- `Info.plist` locks the iPhone target to portrait (R5.landscape deferred).

## Sub-tasks

| ID | Scope | Status |
|---|---|---|
| T1 | SSHClient protocol + `CitadelSSHClient` + `SSHErrorMapping` | ✅ |
| T2 | `Keychain`, `CredentialsStore`, `HostKeyStore` | ✅ |
| T3 | `ConnectionScreen` + `ConnectionViewModel` (form, validation, key import) | ✅ |
| T4 | `HostKeyMismatchScreen` (TOFU warning, fingerprints, trust/reject) | ✅ |
| T5 | `TerminalHostView` SwiftTerm bridge + `TerminalSession` | ✅ |
| T6 | `KeyBar` + `KeyBarController` + `KeyBarState` reducer (R3 keys) | ✅ |
| T7 | Ctrl-pending mode + tick-driven timeout (R4) | ✅ |
| T8 | `DisconnectBanner` + reconnect on `.closed` | ✅ |
| T9 | Dark mode + portrait lock | ✅ |
| T10 | Manual smoke pass: vim, top, ANSI colors, long-line wrap, copy/paste, scrollback | ⏳ |
| T11 | UI review against MVP visual targets (no Figma yet — internal reference) | ⏳ |
| T16 | Lookin Debug Inspector (R12): `LookinServer` (`https://github.com/QMUI/LookinServer`, `from: 1.2.7`) added as a project-level SPM dependency and linked into the `BedTerm` app target's Frameworks build phase. Debug/Release gating happens **inside LookinServer's own Package.swift** via `.when(configuration: .debug)` defines (`SHOULD_COMPILE_LOOKIN_SERVER`), so every `.m` file collapses to an empty translation unit in Release — no source-level changes or imports needed. `OTHER_LDFLAGS = -ObjC` is set on the **Debug** config only, required because LookinServer's classes register via Obj-C `+load` and aren't directly referenced from Swift, so without `-ObjC` the linker dead-strips them. Verification: `nm $APP.app/BedTerm.debug.dylib \| grep -E 'OBJC_CLASS_\$_(Lookin\|LKS)'` reports ~52 classes in Debug and 0 in Release. Engineer usage: run a Debug build on a device or simulator on the same Wi-Fi as the Mac, then open the Lookin Mac app (lookin.work) to attach. | ✅ |
| T14 | Toolbar redesign (R3): strip toolbar down to [ESC] + [CTRL] + [TAB]; remove arrow buttons; rebuild with Liquid Glass material (`.glassEffect()` / `GlassEffectContainer` on iOS 26+) instead of `.bar`; keep `safeAreaInset(edge: .bottom)` hosting. | ✅ |
| T15 | Direction Pad (R11): new `DPad` SwiftUI view stacked above the toolbar via a second `safeAreaInset(edge: .bottom)` (or unified bottom container holding [D-pad, Toolbar] in a `VStack`). Plus-shaped layout with one centre void; each direction is a `Button` wrapped in `LongPressGesture` to drive `KeyBarController.handle(.up/.down/.left/.right)` on press, plus a repeat task while held (≈400 ms initial delay, ≈80 ms cadence). Light haptic on tap, medium on auto-repeat start. Liquid Glass material matching the toolbar. Sized for thumb reach (~64–80 pt per direction button). | ❌ |
| T13a | System Appearance (R9, app chrome): remove forced dark from `BedTermApp`; add `surface.background` / `surface.elevated` / `text.primary` / `text.muted` / `accent` / `accent.label` / `error` / `keybar.background` colour sets to `BedTerm/Assets.xcassets` with Light + Dark values; add a typed `Color` extension in `BedTermKit` exposing the tokens; replace `Color.white` in `KeyBar` with `Color.accentLabel`; audit views for any remaining hard-coded colours. | ❌ |
| T13b | System Appearance (R9, terminal palette) — deferred: bridge `terminal.background` / `terminal.foreground` / `terminal.ansi.*` tokens into a SwiftTerm `TerminalTheme`; rebuild the theme on `traitCollectionDidChange` so the terminal repaints on appearance flip. | ❌ |
| T12 | Local Network permission prewarm (R7): `LocalNetworkPrewarmer` classifies the host as LAN (RFC1918 / ULA / link-local / `.local`) vs loopback vs public. On LAN, drive `NWBrowser` for `_bedterm._tcp` to trigger the iOS prompt and `await` resolution before SSH. Only `NWError.dns(-65555)` (`kDNSServiceErr_PolicyDenied`) is treated as denial — other `.waiting` reasons (no peers found, no network) are treated as granted. `ConnectionScreen` shows "Waiting for local network permission…" while pending; on deny, surfaces Open Settings (`UIApplication.openSettingsURLString`) + Retry; cache grant in `UserDefaults`. Requires `NSBonjourServices` in `Info.plist` — see T17. | ✅ |
| T17 | Explicit `BedTerm/Info.plist` (supersedes `GENERATE_INFOPLIST_FILE`): Xcode 26's `INFOPLIST_KEY_*` allowlist silently drops `NSBonjourServices`, leaving `NWBrowser` unable to trigger the Local Network prompt. Switch the app target to an explicit Info.plist that declares `NSBonjourServices` as an array; `CFBundle*` keys use `$(VAR)` substitutions so they stay tied to build settings. Test targets keep `GENERATE_INFOPLIST_FILE = YES`. | ✅ |
| T18 | First-Launch Onboarding (R10): `OnboardingScreen` (NavigationStack) drives a 2-step picker — host kind (macOS/Other) → location (Same Wi-Fi/Remote) — then branches to a placeholder macOS tutorial (Remote Login + IP lookup; real copy TBD) and/or a Local Network permission step that triggers the prompt eagerly via `LocalNetworkPrewarmer.requestPermission()`. `OnboardingViewModel` persists completion + choices in `UserDefaults`; `BedTermApp` gates root view on `OnboardingViewModel.hasCompleted`. UI tests bypass via `-uitest-skipOnboarding` launch arg. | ⚠️ tutorial copy pending |
| T19 | Multiline Input Composer (R13): `ComposerController` (`@Observable`, owns `text`/`isOpen`, decides bracketed-paste vs raw `\n`→`\r` at submit time) + `ComposePill` floating glass entry + `ComposerBar`. The bar wraps a `ComposerTextView` (`UIViewRepresentable` over `UITextView`) so the rendered content height can drive a collapsed↔expanded morph: capsule single-row pill with `[ text | ✕ | ↑ ]` inline until the buffer wraps past one line, then a continuously growing rounded-rect (cap 112pt; text scrolls beyond) where ✕ moves to the top-trailing corner and ↑ sits at the bottom-trailing corner. Trailing text inset is held constant across the morph so wrapping doesn't oscillate. Monospaced font, autocorrect/autocaps/smart-punctuation/spell-check off, language switching unrestricted. `KeyBar` capsule hugs content (drops `frame(maxWidth:.infinity)`) and pairs with the trailing compose pill (Notes-style leading multi-action + trailing single-action). `TerminalHostView` exposes a `BracketedPasteProbe` reading `terminal.bracketedPasteMode` and yields first responder via `updateUIView` when the composer is open so the system keyboard routes to the composer instead of SwiftTerm. `GlassEffectContainer` + matched `glassEffectID` morph the pill into the input bar. | ✅ |

## Out of scope (tracked in `prd/bedterm/mvp-backlog.md`)

Multi-host list, auto-reconnect, port forwarding, theme settings, custom key bar, iPad layout, ECDSA OpenSSH keys, wrong-passphrase error distinction, R4 `ctrl_not_ctrl` revisit.
