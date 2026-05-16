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
- **All colours come from the asset catalog.** No literal `Color(...)` / `UIColor(red:green:blue:)` / hex values in code or views. Each named colour set in `Assets.xcassets` defines an `Any Appearance` (Light) and a `Dark Appearance` variant; SwiftUI/UIKit resolve them automatically as the trait collection changes. Reference them via `Color("TokenName", bundle: .main)` (or a typed `Color.token` extension generated from the catalog).
  - Define semantic tokens, not raw palette names: e.g. `surface.primary`, `surface.elevated`, `text.primary`, `text.muted`, `accent`, `error`, `keybar.background`, `keybar.label`, `keybar.pendingLatch`, `terminal.background`, `terminal.foreground`, plus ANSI mappings `terminal.ansi.black` … `terminal.ansi.brightWhite`.
  - For SwiftTerm's terminal palette, build a `TerminalTheme` from the resolved `UIColor`s of these tokens. Recompute the theme when `traitCollectionDidChange` reports a `userInterfaceStyle` change (R9.live_switch) so the terminal repaints on appearance flip.
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
| T16 | Lookin Debug Inspector (R12): add `LookinServer` via Swift Package Manager (`https://github.com/QMUI/LookinServer`), gate the dependency so it only links into the Debug configuration (Xcode "Link Binary With Libraries" → set Status to Optional for Release, or use a separate Debug-only target via `.package` condition / `swift-tools-version` traits). No source-level imports needed — LookinServer auto-bootstraps via `+load`. Verify Release archives do not contain `LookinServer.framework` and that strings/symbols search for `Lookin` returns nothing. Document for engineers: run app in Debug on a device or simulator on the same Wi-Fi as the Mac, open the Lookin Mac app to attach. | ❌ |
| T14 | Toolbar redesign (R3): strip toolbar down to [ESC] + [CTRL] + [TAB]; remove arrow buttons; rebuild with Liquid Glass material (`.glassEffect()` / `GlassEffectContainer` on iOS 26+) instead of `.bar`; keep `safeAreaInset(edge: .bottom)` hosting. | ✅ |
| T15 | Direction Pad (R11): new `DPad` SwiftUI view stacked above the toolbar via a second `safeAreaInset(edge: .bottom)` (or unified bottom container holding [D-pad, Toolbar] in a `VStack`). Plus-shaped layout with one centre void; each direction is a `Button` wrapped in `LongPressGesture` to drive `KeyBarController.handle(.up/.down/.left/.right)` on press, plus a repeat task while held (≈400 ms initial delay, ≈80 ms cadence). Light haptic on tap, medium on auto-repeat start. Liquid Glass material matching the toolbar. Sized for thumb reach (~64–80 pt per direction button). | ❌ |
| T13 | System Appearance (R9): remove forced dark; define semantic colour tokens in `Assets.xcassets` with Light/Dark variants; replace every literal colour in code with `Color("...")` lookups; bridge into a SwiftTerm `TerminalTheme` rebuilt on `userInterfaceStyle` change; audit for any remaining hard-coded colours. | ❌ |
| T12 | Local Network permission prewarm (R7): `LocalNetworkPrewarmer` classifies the host as LAN (RFC1918 / ULA / link-local / `.local`) vs loopback vs public. On LAN, drive `NWBrowser` for `_bedterm._tcp` to trigger the iOS prompt and `await` resolution before SSH. Only `NWError.dns(-65555)` (`kDNSServiceErr_PolicyDenied`) is treated as denial — other `.waiting` reasons (no peers found, no network) are treated as granted. `ConnectionScreen` shows "Waiting for local network permission…" while pending; on deny, surfaces Open Settings (`UIApplication.openSettingsURLString`) + Retry; cache grant in `UserDefaults`. Requires `NSBonjourServices` in `Info.plist` — see T17. | ✅ |
| T17 | Explicit `BedTerm/Info.plist` (supersedes `GENERATE_INFOPLIST_FILE`): Xcode 26's `INFOPLIST_KEY_*` allowlist silently drops `NSBonjourServices`, leaving `NWBrowser` unable to trigger the Local Network prompt. Switch the app target to an explicit Info.plist that declares `NSBonjourServices` as an array; `CFBundle*` keys use `$(VAR)` substitutions so they stay tied to build settings. Test targets keep `GENERATE_INFOPLIST_FILE = YES`. | ✅ |
| T18 | First-Launch Onboarding (R10): `OnboardingScreen` (NavigationStack) drives a 2-step picker — host kind (macOS/Other) → location (Same Wi-Fi/Remote) — then branches to a placeholder macOS tutorial (Remote Login + IP lookup; real copy TBD) and/or a Local Network permission step that triggers the prompt eagerly via `LocalNetworkPrewarmer.requestPermission()`. `OnboardingViewModel` persists completion + choices in `UserDefaults`; `BedTermApp` gates root view on `OnboardingViewModel.hasCompleted`. UI tests bypass via `-uitest-skipOnboarding` launch arg. | ⚠️ tutorial copy pending |

## Out of scope (tracked in `prd/bedterm/mvp-backlog.md`)

Multi-host list, auto-reconnect, port forwarding, theme settings, custom key bar, iPad layout, ECDSA OpenSSH keys, wrong-passphrase error distinction, R4 `ctrl_not_ctrl` revisit.
