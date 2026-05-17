# Metal Terminal — Scroll Implementation Spec

PRD rules: `R5.scroll`, `R5.scroll_sync`, `R5.scroll_inertia`, `R5.scroll_snap_on_input` (mvp.xml). Closes bug `metal_viewport_cannot_scroll`; advances `R2M-R1.scrollback`, `R2M-R2.scroll_clean`, `R2M-R4.r5_ux`.

Parent spec: `prd/bedterm/docs/specs/rust-metal-renderer.md`.

## Goal

Touch-scroll the Metal terminal viewport into scrollback. Drag tracks the finger, lifting with velocity decelerates and snaps to whole rows, edges clamp, typing snaps back to live bottom.

## Why Rust owns the offset

`bedterm_core` constructs `Term` via `Dims { total_lines == screen_lines == rows }`, so alacritty's scrollback grid currently holds zero history rows. `bt_term_snapshot` only walks the viewport (`Line(0)..Line(rows)`). There is no historical data on the Swift side to scroll over — Swift cannot fake an offset against state it does not have. The fix is to enable scrollback in the Rust core and expose its display offset.

This also closes `R2M-R1.scrollback` ("Scroll position is exposed to the renderer") in the same step.

## Architecture

```
UIPanGestureRecognizer  ──▶  TerminalMetalUIView.scroll(rowDelta:)
                              │
                              ├─▶ TerminalCore.scrollBy(rowDelta:) ──FFI──▶ bt_term_scroll
                              │                                              │
                              │                                              ▼
                              │                                       Term::scroll_display
                              │                                              │
                              ▼                                              ▼
                          setNeedsDisplay()    ──draw──▶  bt_renderer_draw reads grid
                                                          via display_iter (offset-aware)
```

Snap-on-input path:

```
insertText / pressesBegan / KeyBar / Composer.commit
            │
            ▼
        onSend(bytes)  ── before write ──▶ TerminalCore.scrollToBottom()
            │
            ▼
        SSH.write(bytes)
```

## Rust changes (`rust-core/bedterm_core`)

### `term.rs`

1. **Enable scrollback.** Replace `Dims { cols, rows }` with separate viewport vs history sizing:
   ```rust
   const SCROLLBACK_LINES: u16 = 10_000;
   struct Dims { cols: u16, screen_rows: u16, history_rows: u16 }
   impl Dimensions for Dims {
       fn total_lines(&self)  -> usize { (self.screen_rows + self.history_rows) as usize }
       fn screen_lines(&self) -> usize { self.screen_rows as usize }
       fn columns(&self)      -> usize { self.cols as usize }
   }
   ```
   `Term::new` and `Term::resize` receive `Dims` with `history_rows = SCROLLBACK_LINES`.

2. **Offset-aware snapshot.** Use alacritty's `grid.display_offset()` to translate viewport rows into scrollback-relative line indices:
   ```rust
   pub fn snapshot(&self) -> GridSnapshot {
       let grid = self.term.grid();
       let offset = grid.display_offset() as i32;   // 0 = live bottom
       for row in 0..rows as i32 {
           // Line(0) is the top of the live screen; positive Line values are
           // below; negative values are scrollback. Subtract offset to walk
           // through history when the user scrolled up.
           let line = Line(row - offset);
           // ... read cell ...
       }
   }
   ```
   Cursor row is reported as `cursor.line.0 + offset`; if it falls outside `[0, rows)` (cursor is off-screen because user scrolled past it), the snapshot reports it as `-1` so the Swift cursor layer can hide.

3. **Scroll API.**
   ```rust
   impl Terminal {
       pub fn scroll_by(&mut self, delta: i32);  // +up (into history), -down
       pub fn scroll_to_bottom(&mut self);
       pub fn scroll_offset(&self) -> u32;
       pub fn scrollback_lines(&self) -> u32;    // current history depth, ≤ SCROLLBACK_LINES
   }
   ```
   All four wrap `term.scroll_display(Scroll::Delta(n) | Scroll::Bottom)` and `grid.display_offset()`. `Scroll::Bottom` resets offset to 0.

4. **Auto-follow:** alacritty's default behavior is that incoming output **does not** reset `display_offset` — the cursor advances on the live bottom and history stays where the user left it. That matches `R5.scroll_sync` and requires no extra code.

### `ffi.rs`

Add C exports:

```rust
#[no_mangle] pub extern "C" fn bt_term_scroll_by(t: *mut BtTerm, delta: i32);
#[no_mangle] pub extern "C" fn bt_term_scroll_to_bottom(t: *mut BtTerm);
#[no_mangle] pub extern "C" fn bt_term_scroll_offset(t: *const BtTerm) -> u32;
#[no_mangle] pub extern "C" fn bt_term_scrollback_lines(t: *const BtTerm) -> u32;
```

`cbindgen` regenerates `BedTermCoreC` header on the next xcframework build.

Extend `BtSnapshotView` with one new field — no ABI rename:
```c
uint32_t display_offset;   // 0 = at live bottom; ≤ scrollback_lines
```

`cursor_row` stays `u16`. When the cursor is scrolled off-screen, `Terminal::snapshot()` writes `cursor_row = rows` (one past the last viewport row). Swift's `MetalCursorLayer` already hides the cursor when `row >= rows`, so no Swift change is needed for cursor-hiding.

Exposing `display_offset` in the snapshot lets the Swift inertia loop check edge-clamp without an extra FFI hop per frame.

### Renderer

**No changes.** The renderer pulls grid state via `BtTerm::snapshot_for_renderer()` (in `ffi.rs`), which calls the same `Terminal::snapshot()` that Swift consumes. Applying `display_offset` inside `snapshot()` is sufficient for both. `R2M-R2.scroll_clean` falls out for free once snapshot is offset-aware.

### `ffi.rs` cache invalidation

`BtTerm.cached` is currently invalidated only on `bt_term_feed` and `bt_term_resize`. The two new scroll exports must also `term.cached = None` before returning — otherwise the renderer keeps painting the pre-scroll snapshot.

## Swift changes (`BedTermKit`)

### `TerminalCore.swift`

```swift
public func scrollBy(_ delta: Int) {
    bt_term_scroll_by(handle, Int32(delta))
}
public func scrollToBottom() {
    bt_term_scroll_to_bottom(handle)
}
public var scrollOffset: Int {
    Int(bt_term_scroll_offset(handle))
}
public var scrollbackLines: Int {
    Int(bt_term_scrollback_lines(handle))
}
```

### `TerminalMetalUIView.swift`

#### Gesture install

In `installGestureRecognizers()`:

```swift
let pan = UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:)))
pan.maximumNumberOfTouches = 1
pan.delegate = self        // see arbitration below
addGestureRecognizer(pan)
self.panGR = pan
```

The selection long-press already requires `minimumPressDuration = 0.4`; pan starts firing on the first non-zero translation (≈10 ms), so a normal swipe always wins. A finger held still for 0.4 s before moving fires selection. No `require(toFail:)` needed in either direction — UIKit's natural recognizer arbitration resolves it. Document this as the contract.

#### Pan handler

```swift
private var dragAccumulator: CGFloat = 0   // sub-row remainder
private var displayLink: CADisplayLink?
private var inertiaVelocity: CGFloat = 0   // points/sec, +up

@objc private func handlePan(_ g: UIPanGestureRecognizer) {
    switch g.state {
    case .began:
        stopInertia()
        dragAccumulator = 0
    case .changed:
        let translation = g.translation(in: self).y
        g.setTranslation(.zero, in: self)
        applyScroll(points: -translation)   // drag down = scroll into history = +rows
    case .ended, .cancelled:
        let v = -g.velocity(in: self).y     // points/sec, +up
        if abs(v) > 50 { startInertia(initialVelocity: v) }
    default: break
    }
}

private func applyScroll(points: CGFloat) {
    dragAccumulator += points
    let rows = Int(dragAccumulator / cellSize.height)
    if rows != 0 {
        dragAccumulator -= CGFloat(rows) * cellSize.height
        terminalCore.scrollBy(rows)
        setNeedsDisplay()
    }
}
```

#### Inertia

`CADisplayLink` ticks at the screen's native refresh; on each tick apply `dt`-scaled velocity and decay:

```swift
private func startInertia(initialVelocity: CGFloat) {
    inertiaVelocity = initialVelocity
    displayLink = CADisplayLink(target: self, selector: #selector(tickInertia))
    displayLink?.add(to: .main, forMode: .common)
}
@objc private func tickInertia(link: CADisplayLink) {
    let dt = CGFloat(link.targetTimestamp - link.timestamp)
    applyScroll(points: inertiaVelocity * dt)
    inertiaVelocity *= pow(0.001, dt)   // ~exponential decay to 0.1% per second
    let atTop = terminalCore.scrollOffset >= terminalCore.scrollbackLines
    let atBottom = terminalCore.scrollOffset == 0
    if abs(inertiaVelocity) < 30 || (inertiaVelocity > 0 && atTop) || (inertiaVelocity < 0 && atBottom) {
        stopInertia()
    }
}
private func stopInertia() {
    displayLink?.invalidate()
    displayLink = nil
    inertiaVelocity = 0
}
```

Tap-to-stop: extend the existing `focusTap` handler to also call `stopInertia()`. Since `focusTap.cancelsTouchesInView = false`, tapping the view during deceleration both stops inertia and keeps first-responder behavior.

Edge clamp is enforced by the Rust core (`scroll_display` already clamps to `[0, history_lines]`); the inertia loop checks for boundary contact to stop wasting frames.

No rubber-band overscroll. Mirrors Warp; consistent with terminal-grid semantics where there's no fractional row to spring back from.

#### Snap-on-input

Every UI surface that produces PTY bytes (KeyBar, Composer, hardware keys via `pressesBegan`, soft keys via `UIKeyInput.insertText`/`deleteBackward`) funnels through `TerminalSession.send(_:)` already — see `TerminalScreen.swift` lines 45/53/64 where every `onSend` is `{ session.send($0) }`. So one hook in the session covers all cases:

```swift
// TerminalSession.swift
private(set) var onBeforeSend: (() -> Void)?

func setBeforeSendHook(_ hook: @escaping () -> Void) {
    self.onBeforeSend = hook
}

func send(_ data: Data) {
    guard case .open = state else { return }
    onBeforeSend?()                     // NEW: snap-on-input chokepoint
    Task { try? await client.write(data) }
}
```

The Metal host view registers the hook at construction time:

```swift
// TerminalMetalUIView.init, after the core is created
let core = self.terminalCore
session.setBeforeSendHook { [weak self] in
    guard let self else { return }
    if core.scrollOffset > 0 {
        core.scrollToBottom()
        self.stopInertia()
        self.setNeedsDisplay()
    }
}
```

A weak `session` reference is added to the `TerminalMetalHostView` props next to `feed`/`onSend`/`onResize` so the view can call `setBeforeSendHook`. The SwiftTerm path leaves the hook unset (it manages its own scroll state internally), so the snap is a no-op there.

#### Gesture delegate

```swift
extension TerminalMetalUIView: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ g: UIGestureRecognizer,
                           shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        // Pan should not run simultaneously with selection drag — once selection
        // starts, it owns the gesture. UITapGestureRecognizer is fine in parallel.
        if g === panGR, other === selectionGR { return false }
        return false
    }
}
```

## What does NOT change

- Bottom anchor logic (`scheduleBottomAnchorPass`) — runs against the live bottom row regardless of scroll offset.
- Cursor blink layer — when offset > 0 and snapshot reports `cursor_row_or_negative == -1`, the cursor layer hides; this is the natural behavior of pointing at off-screen content.
- Selection layer — selection coordinates are viewport-relative today. While scrolled, the user can still select what's visible; copying the selection extracts it from the visible snapshot. Selection across scrollback rows is a separate feature, out of scope here.
- ANSI / atlas / shader code.
- SwiftTerm path: untouched.

## Test plan

Unit tests (`bedterm_core/tests`):
1. Feed N rows (N > viewport), assert `scroll_offset == 0`, snapshot cursor is on-screen.
2. `scroll_by(5)` → assert `scroll_offset == 5`, snapshot top row matches what was 5 rows above bottom before.
3. `scroll_by(i32::MAX)` → clamped to `scrollback_lines`.
4. `scroll_by(-i32::MAX)` → clamped to 0.
5. While `scroll_offset > 0`, `feed(b"new line\n")` — assert offset is unchanged (R5.scroll_sync).
6. `scroll_to_bottom()` resets offset to 0.

Swift tests (`MetalRendererBridgeTests` extension):
1. Pan gesture handler called with translation of `5 * cellHeight` → `terminalCore.scrollOffset == 5`.
2. Inertia decays to zero within ~1 s for typical fling velocities (no infinite loop).
3. `send(byte)` while `scrollOffset > 0` calls `scrollToBottom` exactly once before the byte is written.

Manual (run via `worktree-ios-dev` skill):
1. Metal toggle ON. `ls -la /usr/bin` → drag up → scrollback reveals; drag past top → clamps.
2. Release with velocity → decelerates and stops at a whole row.
3. Tap during deceleration → stops immediately.
4. Scroll up halfway through history; type any char → viewport jumps to live bottom in the same frame.
5. Scroll up; toolbar `ESC` tap → also snaps.
6. SwiftTerm toggle OFF — confirm no regression.

## Out of scope (defer)

- "Jump to bottom" floating pill when `scrollOffset > 0` — needs design.
- Sticky command header — depends on R2M-R3 blocks.
- 2-finger swipe between blocks — depends on R2M-R3 blocks.
- Scrollback selection / copy across history rows.
- Configurable scrollback budget (hardcoded 10 000 here).

## Self-review corrections (applied)

1. ~~Renderer needs `display_iter()` switch~~ → not true; renderer already pulls from `Terminal::snapshot()`, so applying offset there is sufficient.
2. ~~`cursor_row` becomes signed~~ → avoid ABI rename; snapshot reports `cursor_row = rows` (one past last viewport row) when cursor is off-screen.
3. ~~View-side `send(_:)` chokepoint plus session-side weak view ref~~ → single hook on `TerminalSession` registered by the view at init; no extra view-state coupling.
4. Added: scroll FFI must invalidate `BtTerm.cached`; `display_offset` exposed in `BtSnapshotView` to avoid per-frame FFI calls from the inertia loop.

## Risk notes

- **Display offset on resize.** alacritty's `Term::resize` may clamp or reset `display_offset` when `screen_lines` grows; verify in test 5 above. If it does, save offset across resize in `Terminal::resize`.
- **Memory.** 10 000 lines × 80 cols × `Cell` (~16 bytes) ≈ 12 MB worst case. Acceptable; consistent with `glyph_atlas_memory` issue in the R2M PRD already budgeting tens of MB.
- **FFI ABI change.** Renaming `cursor_row: u16` to a signed sentinel breaks the existing snapshot consumer signature. Bridge update is mechanical but must land in the same commit; cbindgen will surface it via header regen.
