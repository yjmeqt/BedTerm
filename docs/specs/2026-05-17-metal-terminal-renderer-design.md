# Metal Terminal Renderer (Plan B1) — Design

## Purpose

Replace SwiftTerm's `TerminalView` as the visible renderer with a Metal-backed
view that reads `GridSnapshot`s from the Rust `TerminalCore`. SwiftTerm remains
linked and shipped behind a debug toggle so we can ship Plan B1 without a
big-bang flip. Plan B2 (separate, follow-on) deletes SwiftTerm once Metal has
proven equivalent in production.

This document is Plan A's named successor. It does not introduce OSC 133, block
mode, animation, or any R15/R16 behaviour — those land in Plan C on top of the
new renderer.

## Goals

- A `TerminalMetalView` that renders a terminal grid from `GridSnapshot` at
  60 fps on iPhone 17 Pro, with correct ANSI 16/256/truecolor.
- Feature parity with SwiftTerm for R1–R12 in the modules listed under
  "Parity surface" below. No regression in any green rule.
- A runtime toggle (debug build only, persisted per-host) that flips the
  terminal screen between SwiftTerm (default) and Metal.
- Shadow-harness extension: when Metal is active, the existing
  `TerminalCore` shadow continues to feed the Rust core; the harness adds a
  per-frame snapshot diff against a reference fixture (offline, not in app).

## Non-goals (deferred)

- Removing SwiftTerm from the build (Plan B2).
- OSC 133 / Block Mode / R16 (Plan C).
- Reflow on resize, selection model, link detection (Plan B2).
- Catalyst / macOS slice (already deferred from Plan A).
- Glyph atlas LRU eviction (added in Plan B2 if footprint warrants it).

## Parity surface — what Plan B1 must match

A rule is "matched" when the Metal renderer passes the same XCTest /
manual-validation gate as SwiftTerm does today.

| Area | SwiftTerm capability | Metal requirement |
|---|---|---|
| R2.colour | ANSI 16, 256, truecolor SGR | Same; correctness via fixture-driven snapshot tests |
| R2.cursor | Block / bar / underline + blink | Same; cursor style read from `GridSnapshot.cursorStyle` |
| R5.copy | Long-press text selection → `UIPasteboard` | Same; selection model lives in Swift, not Rust core |
| R5.prompt\_anchored\_bottom | Bottom-anchor pass on size growth | Same hook fires; `TerminalCore.feed` informs anchor |
| R6.scrollback | Pinch / drag through 10k lines, no ghosting | Native; the ghosting bug is the whole reason for the swap |
| R7.dynamic\_type | Re-rasterise glyphs on Dynamic Type change | Glyph atlas invalidates on `traitCollectionDidChange` |
| R8.alt\_screen | `CSI ?1049h/l` transitions | Already handled by `TerminalCore`; renderer just draws |
| R9.bracketed\_paste | Outbound only; UI does not need to know | Unchanged path through `TerminalSession` |
| R10.osc7\_cwd | Title / cwd surfacing | Pass through; no renderer change needed |
| R11.dpad | Direction-pad sends bytes to PTY | Unchanged; renderer is read-only on input |
| R12.bg\_disconnect | Tear down PTY on backgrounding | Unchanged |

Rules NOT covered by Plan B1 (handled elsewhere or deferred):
- R3 toolbar, R4 connection, R13 composer, R14 voice, R15 keyboard dismiss,
  R16 block mode — these are above the renderer; they get the new view
  injected without behaviour changes.

## Architecture

### Layer diagram

```
┌─────────────────────────────────────────────────────────────┐
│ TerminalScreen.swift (SwiftUI)                              │
│   ├── if useMetalRenderer (debug toggle)                    │
│   │     TerminalMetalHostView ─────────┐                    │
│   │                                    │                    │
│   └── else                              │                    │
│         TerminalHostView (SwiftTerm)    │                    │
└─────────────────────────────────────────┼────────────────────┘
                                          │
                       ┌──────────────────▼──────────────────┐
                       │ TerminalMetalHostView (UIViewRep.)  │
                       │   wraps:                            │
                       │     TerminalMetalUIView : MTKView   │
                       │       - terminalCore: TerminalCore  │
                       │       - renderer: MetalRenderer     │
                       │       - selectionLayer (CALayer)    │
                       │       - cursorLayer (CALayer)       │
                       │   subscribes to:                    │
                       │     feed: AsyncStream<Data>         │
                       │   forwards:                         │
                       │     onSend, onResize                │
                       └──────────────────┬──────────────────┘
                                          │
                       ┌──────────────────▼──────────────────┐
                       │ TerminalCore (Plan A)               │
                       │   feed / resize / snapshot          │
                       └─────────────────────────────────────┘
```

### Components (all new unless noted)

1. **`TerminalMetalHostView`** — `UIViewRepresentable`. Mirrors
   `TerminalHostView`'s public surface (`feed:`, `onSend:`, `onResize:`,
   `yieldFirstResponder:`) so `TerminalScreen` can swap them with a single
   branch.

2. **`TerminalMetalUIView : MTKView`** — owns the `TerminalCore`, drives the
   render loop. Implements `UIKeyInput` / `UITextInteractionDelegate` for
   keyboard input, gesture recognisers for selection and scroll. Resize is
   computed from `bounds.size / cellSize` and pushed into `TerminalCore`.

3. **`MetalRenderer`** — `MTKViewDelegate`. Per-frame steps:
   1. Call `terminalCore.snapshot()`.
   2. Hash the snapshot grid + scroll offset + selection. Skip frame if
      unchanged (battery; same trick the existing `TerminalView` doesn't do).
   3. Ensure every codepoint in the viewport has a glyph in the atlas.
   4. Upload one `Cell` GPU buffer + one `UVQuad` buffer (CPU-computed).
   5. Encode a single `drawPrimitives` call: `cols*rows*6` vertices.
   6. Cursor + selection are separate CALayers stacked above the Metal layer
      to avoid recomputing the whole cell buffer on cursor blink.

4. **`GlyphAtlas`** — 2048×2048 R8 texture. Key = `(codepoint, fontStyle)`
   where `fontStyle ∈ {regular, bold, italic, boldItalic}`. Rasterised via
   CoreText. Stored in a flat occupancy grid; no eviction in Plan B1 (added
   in B2 if profiler shows >20MB).

5. **`TerminalMetalView+Selection`** — selection model lives in Swift.
   Tracks `(startRowAbs, startCol, endRowAbs, endCol)` against the absolute
   row index from `GridSnapshot`. Drawn by `selectionLayer` (one
   `CAShapeLayer` per visible row range).

6. **`TerminalScreen`** — modified. Reads
   `@AppStorage("debug.useMetalRenderer")` (debug builds only); production
   builds compile out the toggle and always use SwiftTerm.

### Data flow

PTY bytes arrive as `Data` on the existing `AsyncStream<Data>`. The Metal host
view's coordinator does:

```
for await chunk in feed {
    terminalCore.feed(chunk)
    view.setNeedsDisplay()    // MTKView.isPaused = true; explicit kick
}
```

`MTKView.isPaused = true` and `enableSetNeedsDisplay = true` keep the GPU idle
when nothing changes — important for battery. The cursor-blink timer is the
only timer that runs continuously, and it only invalidates the cursor CALayer.

### Threading

Per Plan A, `TerminalCore` is main-actor-only. The Metal renderer runs on the
main thread for snapshot reads, then hands a copied cell buffer to the GPU.
No background queue for rendering in Plan B1 — measured first, optimised
later.

## Toggle behaviour

- New key: `BedTerm/UserDefaults: debug.useMetalRenderer` (Bool, default
  `false`). Wrapped behind `#if DEBUG` so RELEASE builds compile out the
  whole branch and always use SwiftTerm.
- UI: a row in the existing Connection screen's debug submenu (or wherever
  debug toggles live; check codebase). One switch, "Metal renderer
  (experimental)".
- Switching takes effect on the next session connect. Switching mid-session
  is out of scope — the toggle is observed only when `TerminalScreen`
  appears.

## Glyph atlas details

- Atlas: `MTLTextureDescriptor.texture2DDescriptor(.r8Unorm, 2048, 2048,
  mipmapped: false)`. R8 is enough — colour comes from the fragment shader's
  per-cell `fg`/`bg`.
- Allocation: flat row-stride bin packer. Glyphs are quantised to a fixed
  cell-pixel size (e.g. 32×64 at 3x retina for 14pt SF Mono). Wide chars
  (East Asian, emoji) occupy two adjacent atlas cells.
- Font: Dynamic Type body monospace via `UIFont.monospacedSystemFont(
  ofSize: UIFont.preferredFont(forTextStyle: .body).pointSize)`. Atlas is
  flushed (texture re-allocated, `glyphs` cleared) on `traitCollectionDidChange`
  whose `preferredContentSizeCategory` differs.
- Emoji: rasterised via CoreText `CTFontDrawGlyphs`. Atlas is R8, but emoji
  need RGBA — for Plan B1, emoji fall back to a separate `RGBA8` atlas
  texture sampled by a second fragment-shader branch (selected by
  `cell.width == 2 && cell.codepoint > 0x1F000` heuristic). If profiler
  shows the branch is hot, revisit in B2.

## Cursor and selection

- **Cursor**: a single `CALayer` sublayer of the MTKView. Its frame is
  `CGRect(x: col*cellW, y: row*cellH, w: cellW, h: cellH)` (or 2px high for
  underline style). Blink: 530ms half-cycle `CABasicAnimation` on
  `opacity`. Hidden while the user is scrolled away from the bottom.

- **Selection**: a `CAShapeLayer` sublayer. The path is computed in Swift
  from `(startRowAbs, startCol, endRowAbs, endCol)` against the current
  viewport. Updated on snapshot change (scroll shifts rows) and on touch
  events.

Both layers sit above the MTKView's drawable in the standard layer order, so
they don't trigger a Metal redraw.

## Testing strategy

### Unit tests (XCTest)

- `MetalRendererTests` — feed a fixture into `TerminalCore`, render once to
  a `MTLTexture`-backed offscreen, read pixels back, assert specific cells
  have expected RGBA. Ships with 5 fixtures: SGR colours, 256-colour ramp,
  truecolor block, CJK two-cell render, cursor at viewport bottom.

- `GlyphAtlasTests` — codepoint lookup is idempotent; Dynamic Type change
  triggers a re-rasterise; wide-char glyph occupies two cells.

- `SelectionGeometryTests` — selection rect math for forward / backward /
  multi-row / scrolled selections.

### Shadow / parity tests

- `TerminalRendererParityTests` — for each fixture in
  `BedTermTests/Fixtures/byte_streams/`, run both `TerminalView` (SwiftTerm)
  and the Metal renderer offscreen at the same `MTLDevice` resolution, then
  pixel-compare with a tolerance (≤2 RGB diff per channel). Failures dump a
  side-by-side PNG to the test bundle. This test runs only in CI on the
  simulator; expected to flag font hinting and antialiasing differences,
  which we'll review case by case and add to a known-divergence list.

### Manual gates

- Run `ls --color=always` and confirm 16-colour ANSI matches a SwiftTerm
  reference screenshot.
- Run `tput colors; printf '\e[38;2;255;128;0mHi\e[m\n'` and confirm
  truecolor renders.
- Scroll a 10k-line file via `cat` — no ghosting (the bug class that
  motivated this work).
- Toggle Metal off mid-app, reconnect, confirm SwiftTerm still works
  (no shared mutable state leaked).

## Risks and mitigations

1. **CALayer + MTKView composition** — selection / cursor layers may flicker
   when the drawable presents. Mitigation: present-with-transaction via
   `view.presentsWithTransaction = true`. If that's not enough, fall back
   to drawing cursor/selection inside the fragment shader (more complex,
   one more uniform buffer).

2. **`MTKView` inside `UIViewRepresentable` framing** — SwiftUI's layout pass
   has historically been finicky with `MTKView` reporting drawable sizes
   one frame late. Plan: write a 20-line `SwiftUI` smoke harness early
   (Task 1 of the plan) that just clears to a colour and renders the cell
   count; only proceed past it once layout is stable across rotation and
   keyboard show/hide.

3. **Emoji-as-second-atlas branching** — fragment-shader branch cost.
   Mitigation: profile on iPhone 12 (oldest supported) early; if it
   regresses frame time, fall back to rasterising all glyphs (including
   ASCII) into an RGBA atlas for Plan B1 and revisit footprint in B2.

4. **Dynamic Type re-rasterise stutter** — flushing the atlas on a system
   font-size change rebuilds every glyph. For ~100 unique glyphs typical
   in a terminal session, this is < 50ms on iPhone 12 in informal
   measurement. If it stutters in practice, do lazy re-raster (on next
   miss) instead of eager flush.

5. **Regression in any R1–R12 rule** — the toggle keeps Metal opt-in. We
   ship to TestFlight with Metal off by default and enable it for
   ourselves; any regression is observed before users see it. Plan B2
   waits until the parity test suite is fully green for two consecutive
   weeks of internal use.

## What's in Plan B1 vs Plan B2

| Item | B1 | B2 |
|---|---|---|
| Metal renderer behind toggle | ✅ | already shipped |
| Parity tests vs SwiftTerm | ✅ | retired (SwiftTerm gone) |
| Toggle UI | ✅ | removed |
| Atlas LRU eviction | ❌ | ✅ if needed |
| Reflow on resize | ❌ | ✅ |
| Link detection | ❌ | ✅ |
| Remove SwiftTerm package dep | ❌ | ✅ |
| Delete `TerminalHostView.swift` | ❌ | ✅ |
| Retire shadow harness | ❌ | ✅ |

## Open questions (resolve during plan-writing)

- **Where the debug toggle lives in the UI** — Connection screen's debug
  submenu vs a new dev-tools route. Decide while writing the plan after
  reading `ConnectionScreen.swift`.
- **Whether the parity tests should hard-fail in CI** — start as
  informational (PR comment only); promote to required after two weeks of
  green runs.
- **Glyph cell pixel size at 3x retina** — 14pt SF Mono measured cell is
  ~24×30 logical, so 72×90 physical. Confirm exact metrics with a one-off
  measurement during Task 1.

## Acceptance criteria (Plan B1 done)

- [ ] `TerminalMetalHostView` exists and is selected by the debug toggle.
- [ ] All XCTest unit tests for the renderer pass.
- [ ] Parity tests run in CI on the 5 fixture set; any divergence is
      documented in `docs/specs/rust-terminal-core.md`'s known-divergences
      section.
- [ ] Manual checklist (colour, scroll, cursor blink, selection, copy)
      passes on iPhone 17 Pro simulator and one physical device.
- [ ] No regressions reported in R1–R12 rule statuses in `mvp.xml`.
- [ ] SwiftTerm dependency is still present and the default; toggle off
      restores exact previous behaviour.
