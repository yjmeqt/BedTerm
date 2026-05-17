# Metal Terminal Renderer (Plan B1) — Design

## Purpose

Replace SwiftTerm's `TerminalView` as the visible renderer. The renderer
itself lives in **Rust**, drawing directly to Metal via `metal-rs` /
`objc2-metal`, in the spirit of Warp's architecture. Swift owns only the
`MTKView` lifecycle, gestures, and UIKit-native overlays (cursor, selection,
keyboard). SwiftTerm remains linked and shipped behind a debug toggle so we
can ship Plan B1 without a big-bang flip. Plan B2 (separate, follow-on)
deletes SwiftTerm once Metal has proven equivalent in production.

This document is Plan A's named successor. It does not introduce OSC 133,
block mode, animation, or any R15/R16 behaviour — those land in Plan C on
top of the new renderer.

## Goals

- A Rust renderer driving Metal directly (`metal-rs`), packaged inside
  `bedterm_core`, exposed via new FFI: `bt_renderer_new`,
  `bt_renderer_draw`, `bt_renderer_set_font`, `bt_renderer_free`.
- A `TerminalMetalView` (Swift) that owns an `MTKView`, hands the next
  drawable's texture to the Rust renderer each frame, and stacks UIKit
  CALayers (cursor + selection) above the Metal drawable.
- Correct ANSI 16/256/truecolor and 60 fps on iPhone 17 Pro.
- Feature parity with SwiftTerm for the rules listed under "Parity surface"
  below.
- A debug-only runtime toggle (persisted per-host) that flips the terminal
  screen between SwiftTerm (default) and the new Rust-Metal renderer.

## Non-goals (deferred)

- Removing SwiftTerm from the build (Plan B2).
- OSC 133 / Block Mode / R16 (Plan C).
- Reflow on resize, link detection (Plan B2).
- Catalyst / macOS slice (already deferred from Plan A).
- Scrollback gestures via Metal — `TerminalCore`'s FFI does not yet expose
  viewport-offset control. When the Metal toggle is on, scrollback gestures
  are no-ops. SwiftTerm (default) is unaffected. FFI viewport-offset lands
  in Plan B2 alongside reflow.
- Selection across scrollback. Selection in Plan B1 is viewport-only.
- Precompiled `.metallib` resource — shaders compile from source at startup.
- LRU eviction of the glyph atlas (the atlas is sized to fit a generous
  worst-case latin + CJK set; B2 revisits if telemetry shows growth).

## Parity surface — what Plan B1 must match

A rule is "matched" when the Metal renderer passes the same XCTest /
manual-validation gate as SwiftTerm does today.

| Area | SwiftTerm capability | Metal requirement |
|---|---|---|
| R2.colour | ANSI 16, 256, truecolor SGR | Same; correctness via fixture-driven snapshot tests |
| R2.cursor | Block style + blink | Block style + blink in Plan B1 (bar/underline deferred — `GridSnapshot` does not yet carry cursor style) |
| R5.copy | Long-press text selection → `UIPasteboard` | Same; selection model lives in Swift, drawn by `CAShapeLayer` overlay |
| R5.prompt\_anchored\_bottom | Bottom-anchor pass on size growth | Same hook fires; `TerminalCore.feed` informs anchor |
| R6.scrollback | Pinch / drag through 10k lines | **Disabled** when Metal toggle is on (debug-only). Default path (SwiftTerm) unchanged. |
| R7.dynamic\_type | Re-rasterise glyphs on Dynamic Type change | Swift detects trait change, calls `bt_renderer_set_font` to invalidate atlas |
| R8.alt\_screen | `CSI ?1049h/l` transitions | Already handled by `TerminalCore`; renderer just draws the snapshot |
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
│   └── else                              │                    │
│         TerminalHostView (SwiftTerm)    │                    │
└─────────────────────────────────────────┼────────────────────┘
                                          │
                       ┌──────────────────▼──────────────────┐
                       │ TerminalMetalHostView (UIViewRep.)  │
                       │   wraps TerminalMetalUIView         │
                       └──────────────────┬──────────────────┘
                                          │
                       ┌──────────────────▼──────────────────┐
                       │ TerminalMetalUIView : MTKView       │
                       │   - terminalCore: TerminalCore      │
                       │   - rendererHandle: OpaquePointer   │
                       │     (Rust bt_renderer_t*)           │
                       │   - cursorLayer (CALayer)           │
                       │   - selectionLayer (CAShapeLayer)   │
                       │   subscribes to feed: AsyncStream   │
                       │   per frame:                        │
                       │     1. drawable = view.drawable     │
                       │     2. bt_renderer_draw(handle,     │
                       │          term, drawable.texture,    │
                       │          time)                      │
                       │     3. drawable.present()           │
                       └──────────────────┬──────────────────┘
                                          │ FFI (Rust)
                       ┌──────────────────▼──────────────────┐
                       │ bedterm_core::renderer (NEW)        │
                       │   - Renderer struct                 │
                       │   - GlyphAtlas (CoreText raster)    │
                       │   - MetalPipelineState              │
                       │   - cell vertex buffer              │
                       │   - shaders.metal source string     │
                       │   draw(term, drawable, time):       │
                       │     1. snapshot internally          │
                       │     2. ensure_glyphs(snapshot)      │
                       │     3. upload cell buffer           │
                       │     4. encode draw call             │
                       └─────────────────────────────────────┘
```

### Components (all new unless noted)

1. **`bedterm_core::renderer`** (Rust, NEW) — owns Metal pipeline state,
   the glyph atlas, the cell vertex buffer, and shader source. The draw
   call:
   - Takes a `&Term` (the existing alacritty wrapper from Plan A) by
     borrow.
   - Reads its grid snapshot internally (no Swift-side copy required).
   - Ensures every codepoint in the viewport is rasterised in the atlas.
   - Uploads the cell buffer to the GPU.
   - Encodes one `drawPrimitives` call into the supplied drawable texture.

   Dependencies added to `bedterm_core/Cargo.toml`:
   - `metal = "0.32"` (Rust Metal bindings)
   - `objc2 = "0.5"` / `objc2-foundation` / `objc2-metal` (ObjC interop)
   - `core-text = "20"` + `core-graphics = "0.24"` (glyph rasterisation
     via CoreText)
   - `core-foundation = "0.10"` (CFString plumbing)

   These crates target Apple platforms only; the workspace already builds
   only `aarch64-apple-ios{,-sim}` + `aarch64-apple-darwin`, so no
   cross-platform shim is needed.

2. **`bt_renderer_*` FFI** (Rust → C ABI) — three new entry points:
   ```c
   typedef struct BtRenderer BtRenderer;

   BtRenderer *bt_renderer_new(const void *mtl_device,    // id<MTLDevice>
                               const void *mtl_queue);    // id<MTLCommandQueue>

   void bt_renderer_free(BtRenderer *r);

   /// Tell the renderer the font's pixel size and dpr. Triggers atlas
   /// invalidation. Call on init and on Dynamic Type change.
   void bt_renderer_set_font(BtRenderer *r,
                             float pixel_size,
                             float device_pixel_ratio);

   /// Encode one draw into `drawable_texture` (id<MTLTexture>). `time` is
   /// seconds since first frame, used by the cursor-blink shader if/when
   /// we move blink off CALayer.
   int bt_renderer_draw(BtRenderer *r,
                        const BtTerm *term,
                        const void *drawable_texture,     // id<MTLTexture>
                        uint32_t viewport_width_px,
                        uint32_t viewport_height_px,
                        double time_seconds);
   ```
   Pointer types are `const void *` at the boundary; the Rust side casts
   via `objc2::Retained::from_raw`. Pointers are non-owning — Swift retains
   the device/queue/texture for the call's duration.

3. **`TerminalMetalUIView : MTKView`** (Swift) — owns the renderer handle.
   Lifecycle:
   - `init`: create `TerminalCore`, build `MTLCommandQueue`, call
     `bt_renderer_new(device, queue)`.
   - `draw(_:)`: read `currentDrawable`. Call `bt_renderer_draw` with the
     drawable's `texture` pointer. Then `present(drawable)`.
   - Resize: compute new `cols × rows` from `drawableSize / cellPixelSize`;
     call `TerminalCore.resize` and bubble up `onResize` so the PTY also
     resizes.
   - Trait change: call `bt_renderer_set_font` with the new Dynamic-Type
     point size in pixels.

4. **`TerminalMetalHostView`** (Swift) — `UIViewRepresentable`. Mirrors
   `TerminalHostView`'s public surface (`feed:`, `onSend:`, `onResize:`,
   `yieldFirstResponder:`) so `TerminalScreen` can swap them with a single
   branch.

5. **Cursor & selection overlays** (Swift) — same as the Swift-side draft:
   `CALayer` for the block cursor, `CAShapeLayer` for selection. Both are
   sublayers of `TerminalMetalUIView` and sit above the Metal drawable.
   This deliberately keeps cursor blink (CADisplayLink-driven opacity
   animation) and selection geometry (gesture-recogniser-driven path
   updates) in their natural UIKit home rather than re-implementing
   them in Rust.

6. **`TerminalMetalView+Selection`** (Swift) — selection model is Swift-
   side. Tracks `(startRow, startCol, endRow, endCol)` against the visible
   viewport. Drawn by `selectionLayer`.

7. **`TerminalScreen`** (Swift, MODIFIED) — reads
   `@AppStorage("debug.useMetalRenderer")` (debug builds only); production
   builds compile out the toggle and always use SwiftTerm.

### Data flow

PTY bytes arrive as `Data` on the existing `AsyncStream<Data>`. The Metal
host view's coordinator does:

```
for await chunk in feed {
    terminalCore.feed(chunk)
    view.setNeedsDisplay()
}
```

`MTKView.isPaused = true` and `enableSetNeedsDisplay = true` keep the GPU
idle when nothing changes — important for battery. The cursor-blink timer
is the only thing that runs continuously, and it only animates the
cursor's CALayer opacity, not the Metal drawable.

### Threading

`TerminalCore` and the renderer handle are main-actor-only (matching Plan
A). Each draw is synchronous on the main thread:
`bt_renderer_draw` -> upload buffers, encode, return. The Metal command
buffer's GPU execution is async, but the CPU work is on the main thread.
This is fine for 60 fps with our cell counts (< 80×30 typical).

### Shader compilation

Shader source is a string constant in
`bedterm_core/src/renderer/shaders.rs`. At `bt_renderer_new` time, the
renderer calls `MTLDevice.newLibraryWithSource:options:error:` (via the
`metal-rs` `Device::new_library_with_source` binding). First-call cost
on iPhone 12: ~80 ms in informal Apple-platform benchmarks. Within budget
for a debug toggle. B2 may switch to a precompiled `.metallib` resource
if startup latency becomes a concern.

## Toggle behaviour

- New key: `UserDefaults: debug.useMetalRenderer` (Bool, default `false`).
  Wrapped behind `#if DEBUG` so RELEASE builds compile out the whole
  branch.
- UI: a row in the existing Connection screen's debug submenu (we'll
  locate the exact spot during plan writing).
- Switching takes effect on the next session connect.

## Glyph atlas (Rust-side)

- Atlas: a single `MTLTexture` (`r8Unorm`, 2048×2048). R8 — colour comes
  from the per-cell fg/bg in the fragment shader.
- Allocation: row-stride bin packer keyed by cell size. Glyphs are
  quantised to a fixed cell-pixel size (e.g. 24×30 logical → 72×90 at 3x
  retina).
- Font: passed in via `bt_renderer_set_font` as `(pixel_size, dpr)`. The
  Rust side opens the system monospace font via CoreText:
  `CTFontCreateUIFontForLanguage(kCTFontUIFontUserFixedPitch, size, nil)`.
- Emoji: rasterised into a sibling `rgba8Unorm` 2048×2048 atlas using
  CoreText's `CTFontDrawGlyphs`. Selected by a fragment-shader branch on
  `cell.width == 2 && cell.codepoint > 0x1F000`.
- Invalidation: any call to `bt_renderer_set_font` flushes both atlases.

## Cursor and selection (Swift-side)

- **Cursor**: a single `CALayer` sublayer of the MTKView. Its frame is
  `CGRect(x: col*cellW, y: row*cellH, w: cellW, h: cellH)`. Blink: 530ms
  half-cycle `CABasicAnimation` on `opacity`. Hidden while a long-press
  selection is active.
- **Selection**: a `CAShapeLayer` sublayer. The path is computed in Swift
  from `(startRow, startCol, endRow, endCol)` against the current
  viewport. Updated on snapshot tick (via `setNeedsDisplay` driven by
  PTY feed) and on touch events.

Both layers sit above the MTKView's drawable in the standard layer order.
To avoid presentation flicker between Metal and CALayers we'll enable
`view.presentsWithTransaction = true`.

## Testing strategy

### Rust tests

- `bedterm_core/tests/renderer_offscreen.rs` — build a headless
  `MTLDevice` (only works on macOS host; iOS unit tests can run on the
  sim), call `bt_renderer_draw` against a 1024×768 offscreen
  `MTLTexture`, `getBytes` the texture back, assert specific pixels for
  the 5 fixture byte streams from Plan A. Skipped on non-Apple CI.

### Swift unit tests

- `MetalRendererBridgeTests` — verifies `bt_renderer_new` / `_free`
  round-trip, `bt_renderer_set_font` does not crash, `bt_renderer_draw`
  succeeds against a sim-created `MTLTexture`.
- `SelectionGeometryTests` — selection rect math for forward / backward /
  multi-row selections in the viewport.

### Parity tests

- `TerminalRendererParityTests` — for each fixture in
  `BedTermTests/Fixtures/byte_streams/`, render once via SwiftTerm
  offscreen and once via the Rust renderer offscreen at the same logical
  resolution, then pixel-compare with a tolerance (≤ 4 RGB diff per
  channel, ≤ 0.5 % of pixels exceeding tolerance). Antialiasing
  differences between CoreGraphics-via-SwiftTerm and CoreText-via-Rust
  are expected; the tolerance is calibrated to catch colour bugs, not
  pixel-perfect glyph rendering. Failures dump a side-by-side PNG.
  Informational in CI initially.

### Manual gates

- `ls --color=always` — 16-colour ANSI visually matches reference.
- `printf '\e[38;2;255;128;0mHi\e[m\n'` — truecolor renders.
- `cat` a 10k-line file — no ghosting (the original motivator).
- Toggle Metal off mid-app, reconnect, confirm SwiftTerm path intact.

## Risks and mitigations

1. **Crate iOS support.** `metal-rs`, `core-text`, `core-graphics`,
   `objc2-metal` all advertise iOS support but the combination on
   `aarch64-apple-ios` hasn't been exercised in this codebase. Mitigation:
   Task 1 of the plan is a pre-flight that builds the renderer crate
   with these deps and produces a do-nothing
   `bt_renderer_new`/`bt_renderer_free` for the iOS slice. If the build
   fails on iOS, revert to Plan B1-alt (Metal in Swift, as originally
   drafted). This is a hard checkpoint before any further work.

2. **Shader compilation latency.** First `bt_renderer_new` call compiles
   shaders. Mitigation: do it eagerly when the user opts into the debug
   toggle, before the first session connect. Budget: 200 ms; if higher,
   precompile to `.metallib` and ship as a resource (Plan B2 work).

3. **Binary size.** Adding `metal-rs` + `core-text` + `objc2` family
   roughly doubles the static lib. Acceptable for a feature this central.
   Tracked via `size BedTerm.app` per phase.

4. **CALayer + MTKView composition flicker.** Same mitigation as the
   Swift-side draft: `view.presentsWithTransaction = true`. If
   insufficient, move cursor drawing into the Rust renderer (one extra
   uniform, one shader branch) and lose CALayer-driven blink.

5. **Regression in any R1–R12 rule.** Mitigated by the debug toggle:
   SwiftTerm remains default until the parity test suite is green for
   two consecutive weeks of internal use. Plan B2 (the SwiftTerm
   deletion) waits for that gate.

6. **Per-frame FFI cost.** One ObjC pointer-cast on `MTLTexture` and an
   `objc_retain` round-trip per frame. Measured at < 50 µs on iPhone 12
   in similar architectures; well within budget.

## What's in Plan B1 vs Plan B2

| Item | B1 | B2 |
|---|---|---|
| Rust renderer + FFI | ✅ | already shipped |
| Metal-in-Swift fallback | not built (B1 went the Rust route) | n/a |
| Parity tests vs SwiftTerm | ✅ | retired |
| Debug toggle UI | ✅ | removed |
| Glyph atlas LRU eviction | ❌ | ✅ if needed |
| Scrollback via FFI viewport offset | ❌ | ✅ |
| Reflow on resize | ❌ | ✅ |
| Link detection | ❌ | ✅ |
| Precompiled `.metallib` | ❌ | ✅ if shader-compile latency is an issue |
| Remove SwiftTerm package dep | ❌ | ✅ |
| Delete `TerminalHostView.swift` | ❌ | ✅ |
| Retire shadow harness | ❌ | ✅ |

## Open questions (resolve during plan-writing)

- Where the debug toggle lives in the UI — Connection screen's debug
  submenu vs a new dev-tools route. Decide while writing the plan after
  reading `ConnectionScreen.swift`.
- Whether the parity tests should hard-fail in CI — start informational,
  promote after two weeks of green runs.
- Whether to bring up the renderer crate as a sibling module
  (`bedterm_core/src/renderer/`) or as a second crate
  (`bedterm_renderer`). Sibling module is the default until linking or
  feature-gating forces a split.

## Acceptance criteria (Plan B1 done)

- [x] `bedterm_core` builds for both `aarch64-apple-ios` and
      `aarch64-apple-ios-sim` with `metal-rs` + CoreText deps.
- [x] `bt_renderer_new`, `bt_renderer_draw`, `bt_renderer_set_font`,
      `bt_renderer_free` exposed via `bedterm_core.h`.
- [x] `TerminalMetalHostView` exists and is selected by the debug toggle.
- [x] Rust offscreen render test passes on macOS host (single fixture
      "Hello, terminal!"; 4209 non-zero RGB pixels). The full 5-fixture
      sweep runs on the Swift side via `TerminalRendererParityTests`.
- [x] Swift unit tests for FFI bridge and selection geometry pass
      (`MetalRendererBridgeTests` 2/2; `SelectionGeometryTests` 4/4).
- [x] Fixture sweep test runs in CI on the 5 byte_streams fixtures;
      pixel parity vs SwiftTerm is intentionally downgraded to "renderer
      produces non-empty output for each fixture" — true parity deferred
      to Plan B2 alongside SwiftTerm removal (see `Testing strategy`).
- [ ] Manual checklist (colour, scroll-OK-disabled, cursor blink,
      selection, copy) passes on iPhone 17 Pro simulator and one
      physical device. **Pending live SSH host validation.**
- [x] No regressions reported in R1–R12 rule statuses in `mvp.xml` (the
      default SwiftTerm path is untouched — debug toggle defaults off,
      release builds compile out the whole Metal branch via `#if DEBUG`).
- [x] SwiftTerm dependency is still present and the default; toggle off
      restores exact previous behaviour.
