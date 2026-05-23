# Background Sessions — iOS Implementation Spec

PRD: `prd/bedterm/background-sessions.xml`. This document describes **how** the multi-session model is built on iOS. Behavior lives in the PRD.

## What changes, at a glance

- Today's `HostsViewModel.lastSession` (one slot) becomes a `SessionRegistry` (a list per host, plus a "foreground" pointer).
- `TerminalSession` ownership moves out of the terminal screen — the screen becomes a thin view over a registry-owned session. Popping the screen no longer destroys the session; an explicit `kill` call does.
- A new `SessionsPanel` screen lists sessions for one host. Hosts row-body taps route here (the Connect button still starts a new session directly).
- A new `KilledSessionDetailScreen` renders the frozen `BlockStore` of a killed session and exposes "Resume here" / "New shell" actions.
- The terminal top bar replaces the single "Disconnect" with `←` Back and `×` Kill.
- OSC 133 prompt events already carry CWD (per `Osc133Sniffer` in the Rust core). We surface the latest captured value on the session record.

## Architecture

```
SessionRegistry  (@Observable, @MainActor)
   ├─ sessions: [SessionRecord]
   └─ foregroundID: SessionRecord.ID?

SessionRecord  (@Observable)
   ├─ id: UUID
   ├─ hostID: SavedHost.ID
   ├─ state: .running | .killed(reason: KillReason, at: Date)
   ├─ session: TerminalSession?         // nil after kill
   ├─ snapshot: BlockStore?              // populated on kill
   ├─ lastCwd: String?
   ├─ lastCommand: String?
   ├─ lastExitCode: Int32?
   ├─ lastActivity: Date
   └─ hasUnread: Bool                    // R2.row_unread_indicator
```

`HostsViewModel` keeps `displayName(for:)` and the connect flow, but delegates session storage to `SessionRegistry`. The `currentSessionID` / `lastSession` / `sessionEnded` / `confirmSwap` API surface collapses into registry calls.

### Routing

`AppRoute` gains two cases:

```swift
enum AppRoute: Hashable {
    case hostForm(SavedHost.ID?)
    case sessionsPanel(SavedHost.ID)        // NEW
    case terminal(SessionRecord.ID)          // changed: now carries session id
    case killedSessionDetail(SessionRecord.ID) // NEW
    case hostKeyMismatch(...)                // unchanged
}
```

`.terminal` becoming `.terminal(SessionRecord.ID)` means the same route can address any session. Popping `.terminal(id)` no longer triggers `sessionEnded()`; it just changes navigation. Killing a session is explicit.

### Hosts list integration

`HostRow` change: the row body is no longer inert. Tapping the body pushes `.sessionsPanel(host.id)` (R2 entry). Tapping the Connect button creates a new running session and pushes `.terminal(newSessionID)`.

Supersedes `Hosts R2.single_live_session`: the per-host swap-confirm dance is removed. Connect always creates a sibling session. Cross-host caps are governed by `R7.running_session_cap`.

### Lifecycle of a running session

1. **Create.** `SessionRegistry.startSession(for: hostID)` runs `ConnectAttempt`, on `.session(s)` appends a new `SessionRecord(state: .running, session: s)`, sets `foregroundID`, returns the id. Caller pushes `.terminal(id)`.
2. **Background.** User taps `←` Back. Terminal screen pops. `SessionRecord` stays in registry; `session.pumpTask` keeps draining the SSH channel into `TerminalCore`. The Metal view is unmounted but `TerminalCore` lives on because the record retains it.
3. **Re-foreground.** User taps a running row in `SessionsPanel`. Push `.terminal(record.id)`. The screen mounts a fresh `TerminalMetalUIView` bound to the existing `terminalCore` and starts consuming `session.feed` from wherever it is. First frame paints the current grid.
4. **Kill.** User taps `×` (with confirmation) — or `Discard` on a panel row, or `swap_evicted` from R7. `record.session?.disconnect()`; freeze `record.snapshot = session.blockStore.clone()`; capture `lastCwd`, `lastCommand`, `lastExitCode` (already on the record from OSC 133); set `state = .killed(.user_killed, at: .now)`. Clear `record.session` to release the SSH client. Pop to `.sessionsPanel(hostID)`.

### TerminalCore detachment

Today the survey noted `TerminalCore` is "tightly bound to MTKView". Concretely: `TerminalMetalUIView` creates the `TerminalCore` in its initializer and writes it back onto `session.terminalCore` (weak). When the view is removed from the hierarchy, ARC drops the core.

Required change: **`TerminalSession` owns `TerminalCore` strongly**. The view borrows it (passed in via initializer) rather than constructing it. On view tear-down, the core remains alive on the session. On re-foreground, a new view is constructed against the same core.

Smallest change: add a strong `var core: TerminalCore?` on `TerminalSession`, populate it in `connect(...)` (Rust-side `bt_term_new(...)`), and refactor `TerminalMetalUIView.init` to accept an externally-owned core instead of creating its own.

The feed pump (`pumpTask`) already lives on `TerminalSession`. With strong core ownership, the pump can call `core.feed(chunk)` directly — keeping the Rust scrollback up to date without a mounted view. That is what makes "running in background" real.

### CWD capture

`Osc133Sniffer` already emits prompt-start events with `pwd`. Today nothing reads them outside the block store. Add to `TerminalSession` (called from the same place as `blockStore.refresh`):

```swift
func ingestOsc133(events: [Osc133Event]) {
    for e in events {
        switch e {
        case .promptStart(let pwd, _):
            self.lastCwd = pwd                          // R5.cwd_from_osc133
        case .commandFinished(let cmd, let exit, _):
            self.lastCommand = cmd
            self.lastExitCode = exit
        default: break
        }
        self.lastActivity = .now
        self.hasUnread = true
    }
}
```

The `SessionRecord` mirrors these onto itself in an `onChange(of: session.lastActivity)`.

### Sessions panel UI

`SessionsPanelScreen`:

- SwiftUI `List` over `registry.sessions(for: hostID)` sorted by `R1.ordering_by_recency`.
- Row: status badge (`SessionStatusBadge` — filled / hollow / error-filled dot), three-line text (last command, abbreviated CWD, relative timestamp), unread dot when applicable.
- Trailing toolbar: `+` New session.
- Leading swipe: `Discard` (kill + remove for running, drop snapshot for killed).
- Empty state when host has zero sessions.

`KilledSessionDetailScreen`:

- Same block-rendering view component the live terminal uses, but driven from the frozen `BlockStore` snapshot. Read-only — no composer, no key bar, no direction pad.
- Header: host name, "Killed Xm ago", kill-reason copy, last CWD.
- Primary "Resume here" → `registry.resumeKilled(record.id)` which starts a new session and queues `cd <quoted-cwd>\n` to be written to the PTY on the first prompt.
- Secondary "New shell" → same but without the queued `cd`.

### Top-bar redesign

`TerminalScreen` toolbar replacement:

```swift
.toolbar {
    ToolbarItem(placement: .topBarLeading) {
        Button { dismiss() } label: { Image(systemName: "chevron.left") }
            .accessibilityLabel(String(localized: "Back"))
    }
    ToolbarItem(placement: .principal) {
        Button { router.push(.sessionsPanel(record.hostID)) } label: {
            VStack { Text(hostName).font(...); Text(sessionCount).font(.caption) }
        }
    }
    ToolbarItem(placement: .topBarTrailing) {
        Button(role: .destructive) { showKillConfirm = true } label: {
            Image(systemName: "xmark").foregroundStyle(Color("error"))
        }
        .accessibilityLabel(String(localized: "Kill session"))
    }
}
```

`dismiss()` pops the navigation stack without touching the registry — that is the entire "background it" behavior. The kill confirmation calls `registry.kill(record.id, reason: .user_killed)` which clears the session, freezes the snapshot, and pops.

### Resource caps (R7)

Implemented inside `SessionRegistry.startSession`:

- Count `running` sessions across all hosts. If `>= 4`, find the oldest by `lastActivity` and call `kill(_, reason: .swap_evicted)` first, then proceed.
- After appending a new `killed` snapshot in `kill`, enforce per-host killed-snapshot count <= 10 FIFO.

Persistence is out of scope for v1 (R7.snapshots_in_memory_only_mvp). Cold start = empty registry.

## File-level surgery checklist

- **Add** `BedTermKit/Sources/BedTermKit/Features/Sessions/SessionRegistry.swift`
- **Add** `BedTermKit/Sources/BedTermKit/Features/Sessions/SessionRecord.swift`
- **Add** `BedTermKit/Sources/BedTermKit/Features/Sessions/SessionsPanelScreen.swift`
- **Add** `BedTermKit/Sources/BedTermKit/Features/Sessions/KilledSessionDetailScreen.swift`
- **Add** `BedTermKit/Sources/BedTermKit/Features/Sessions/SessionStatusBadge.swift`
- **Add** localized strings: `Back`, `Kill session`, `End this session?`, `End`, `Cancel`, `Sessions`, `New session`, `Discard`, `Resume here`, `New shell`, `Killed`, `you ended this session`, `server closed the connection`, `network drop`, `Replaced by a newer session`. (Per CLAUDE.md i18n rules — SwiftUI literals auto-extract; non-SwiftUI uses `String(localized:)`.)
- **Modify** `BedTermKit/Sources/BedTermKit/Features/Terminal/TerminalSession.swift` — strong `core` ownership, `ingestOsc133`, `lastCwd`/`lastCommand`/`lastExitCode`/`lastActivity`/`hasUnread`, `cloneSnapshot()`.
- **Modify** `BedTermKit/Sources/BedTermKit/Features/Terminal/Metal/TerminalMetalUIView.swift` — accept externally-owned `TerminalCore` instead of constructing one.
- **Modify** `BedTermKit/Sources/BedTermKit/Features/Terminal/TerminalScreen.swift` — top bar (Back / title / ×), drive from `SessionRecord`.
- **Modify** `BedTermKit/Sources/BedTermKit/Features/Hosts/HostsViewModel.swift` — drop the single-session slot; delegate to `SessionRegistry`. Keep `displayName(for:)`.
- **Modify** `BedTermKit/Sources/BedTermKit/Features/Hosts/HostRow.swift` — row body taps push the sessions panel.
- **Modify** `BedTerm/App/AppRoute.swift` — new cases.
- **Modify** `BedTerm/App/BedTermApp.swift` — inject `SessionRegistry`, route new cases.

Existing tests that depend on `HostsViewModel.lastSession` / `currentSessionID` / `confirmSwap` will need migration to `SessionRegistry` equivalents.

## Phased delivery

The PRD is large enough that landing it in one PR risks subtle regressions. Recommended phasing:

| Phase | Scope | Shipping value |
|---|---|---|
| **P1** | TerminalCore strong-ownership refactor (no behaviour change). `TerminalSession` gains `lastCwd` etc., wired to OSC 133. | Foundation; nothing user-visible. |
| **P2** | `SessionRegistry` + top-bar rename (Back / ×). Back still kills + pops for now (snapshot saved). `SessionsPanelScreen` shows killed sessions. `KilledSessionDetailScreen` with Resume here. | Marquee value — preserved scrollback + resume into last CWD. |
| **P3** | True background-running: Back no longer kills; `SessionRegistry` keeps `TerminalSession` alive; re-foreground attaches a fresh view onto the existing core. | Warp-style multi-session. |
| **P4** | Resource caps (R7), unread badge polish. | Production-ready. |

## Automated validation

- **Unit tests** (BedTermTests):
  - `SessionRegistryTests` — start, kill, swap_evicted, killed-cap FIFO, per-host scoping.
  - `OSC133CwdCaptureTests` — feed a known prompt sequence, assert `lastCwd` updates.
  - `KillReasonClassifierTests` — map SSH disconnect causes to `KillReason`.
- **UI test** (BedTermUITests):
  - `BackgroundSessionFlowTests` — drive a debug-mock-tty session (per `prd/bedterm/debug-mock-tty.xml`), tap Back, return via panel, kill via ×, open killed detail, tap "Resume here", verify a new running session appears.
- **Manual / simulator E2E** via `worktree-ios-dev` skill against the iPhone 17 sim using the debug mock TTY so we don't need a real SSH host. Automated by `xcodebuildmcp-cli` UI driver: boot sim → install → launch → mock-host → run a `cd /tmp && ls` → Back → tap host row → see panel → tap × → confirm → see killed → tap row → see scrollback → tap Resume here → verify new session lands in `/tmp`.

## Open design questions worth flagging before P3

1. **Live-feed cost while backgrounded.** A `claude` session in alt-screen can emit kilobytes a second. While the Metal view is unmounted we still drain into the Rust scrollback — bounded ring buffer caps RAM, but battery and SSH keepalive are not free. If this matters, P3 can gate output drain on a "user expects this to keep running" signal and otherwise rate-limit.
2. **Resize-on-foreground TUIs.** R6.foreground_resizes_on_geometry_change issues a single `SIGWINCH` if geometry changed. Some TUIs (vim, claude) handle this well; some legacy programs (`less` without `-R` smarts) repaint poorly. Acceptable for v1.
3. **Bracketed paste of `cd`.** R5.cwd_resume_quoting wraps the path; we also need to decide whether `cd` is sent via `send_uses_bracketed_paste` (Input Bar R4) or as raw bytes. Raw bytes are simpler and avoid double-quoting; the path lives entirely on the first line so bracketed paste buys nothing.
