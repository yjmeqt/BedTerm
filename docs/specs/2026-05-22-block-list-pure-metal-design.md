# Block List — Pure Metal Refactor

**Status:** Draft
**Owner:** yi.jiang
**Branch:** `feature/block-list-pure-metal`
**Aligns with:** Warp's `block_list_element.rs` single-pass paint model

## 1. Motivation

`BlockListContainerView` today is a three-layer sandwich:

- `UIScrollView` + `contentView` holds per-block `UIHostingController<BlockHeader>` strips at absolute Y.
- `metalView` (sibling, beneath the scroll view) paints all block bodies in one Rust draw call.
- `pinnedHost: UIHostingController<BlockHeader>` + `pinnedBackground: UIView` overlay the metal view to implement the sticky/section-header behavior.

Warp does not work this way. In `app/src/terminal/block_list_element.rs` everything — block bodies, per-block "snackbar" headers, the pinned sticky header, dividers — lives in one Metal scene. The header rect is recomputed every frame by `SnackbarHeader::update()`, drawn via `scene.draw_rect_with_hit_recording`, and the body grid is rendered into a `scene.start_layer(ClipBounds::…)` so output cannot bleed under the pinned header band.

We refactor the iOS block list to the same shape: one Metal pass, headers drawn in Rust, no SwiftUI hosting per block.

## 2. Goals & non-goals

**Goals**

- Single `bt_renderer_draw_block_list` call per frame paints bodies + per-block header bands + sticky band + dividers.
- No `UIHostingController<BlockHeader>` per block; no `pinnedHost`; no `pinnedBackground` UIView.
- Scroll input switches to `UIPanGestureRecognizer` with hand-written momentum mirroring Warp's `MOMENTUM_DECAY = 0.968` model.
- Header sticky math (currently in `BlockListContainerView+Sticky.swift`) ports verbatim, but now emits a renderer descriptor instead of mutating a hosting controller.
- Text selection (`BlockListSelectionController`) keeps working — long-press + drag, same hit-resolution behaviour.

**Non-goals (v1)**

- Scroll-to-top status-bar tap. UIScrollView gave this for free; we lose it. Acceptable per Warp parity.
- VoiceOver focus on individual headers (a follow-up — we'd synthesize `accessibilityElements` from descriptors).
- Rubber-band overscroll. We already disable it (`bounces=false`); the new path also clamps hard, matching Warp.
- Scroll indicators. Warp doesn't show them in the block list; we drop them.
- Bidi / complex shaping in header text beyond what cosmic-text gives us out of the box (terminal grid has the same limit).

## 3. Architecture

### 3.1 Swift container

`BlockListContainerViewController` becomes two siblings inside `view`:

- **`inputView: UIView`** — full-frame, transparent, `isUserInteractionEnabled = true`. Owns `panGR` (scroll) and `longPressGR` (selection). Hit-tests pass through to nothing else because nothing else accepts touches.
- **`metalView: TerminalBlocksMetalView`** — full-frame, beneath `inputView`. Paints everything visible.

Removed:

- `scrollView: UIScrollView`
- `contentView: UIView`
- `headerHosts: [UInt64: UIHostingController<BlockHeader>]`
- `dividerHosts: [UInt64: UIView]`
- `pinnedHost: UIHostingController<BlockHeader>?`
- `pinnedBackground: UIView?`
- `pinnedBlockID: UInt64?`

New state on the controller:

```swift
private(set) var contentOffsetY: CGFloat = 0      // replaces scrollView.contentOffset.y
private var contentHeight: CGFloat = 0            // replaces scrollView.contentSize.height
private var momentum: MomentumState?              // nil when not animating
private var lastSamples: [(t: CFTimeInterval, y: CGFloat)] = []  // ring of 3 for velocity
```

`scrollPosition: ScrollPosition` (the `.followsBottom` / `.fixedAt` enum we already use) is retained — it's the semantic state model we keep from Warp's `ScrollPosition` enum and our existing implementation.

### 3.2 Scroll input — `UIPanGestureRecognizer` + custom decel

`panGR` handler on `inputView`:

| Phase | Action |
|-------|--------|
| `.began` | Stop any in-flight momentum. Snapshot `startOffsetY = contentOffsetY`. Clear samples. |
| `.changed` | `contentOffsetY = clamp(startOffsetY - translation.y, 0, maxOffset)`. Append `(CACurrentMediaTime(), contentOffsetY)` to samples (keep last 3). Mark layout dirty; the next display tick redraws. |
| `.ended` / `.cancelled` | Compute velocity from the last two samples: `v = (y₁ - y₀) / (t₁ - t₀)`. If `|v| > 50` px/s, start `MomentumState { v: v, lastTick: CACurrentMediaTime() }`. |

Momentum animator runs on the existing `CADisplayLink`:

```
dt = now - momentum.lastTick
contentOffsetY = clamp(contentOffsetY + momentum.v * dt, 0, maxOffset)
momentum.v *= pow(0.968, dt / 0.008)         // Warp's decay
if |momentum.v| < 1 px/s or we clamped, momentum = nil
```

`pow(0.968, dt / 0.008)` keeps the decay frame-rate-independent (Warp samples at fixed 8ms; we run at display refresh).

`CADisplayLink` lifecycle: active whenever (a) any block is running, (b) momentum is live, or (c) layout has been marked dirty. Today it only tracks (a) — extend `updateDisplayLink()` to consider all three.

Gesture coexistence:

- `panGR.delegate = self`; `gestureRecognizer(_:shouldRecognizeSimultaneouslyWith:)` returns `false` for `longPressGR`. Pan wins on movement; long-press fires only on still-finger. Mirrors today's UX.
- `panGR.cancelsTouchesInView = false` so taps inside future header buttons (out of v1 scope) aren't eaten.
- Accessibility scroll: override `accessibilityScroll(_:)` on `inputView` to page by `viewHeight * 0.8` with a 0.25s `UIView.animate` tween of `contentOffsetY`. Stays VoiceOver-accessible.

### 3.3 Rust renderer additions

#### 3.3.1 FFI surface

Extend the existing `bt_renderer_draw_block_list` (or add `_v2` and delete the old in the same PR — see §7):

```c
typedef struct {
    uint64_t block_id;
    float    header_y_top_px;
    float    header_height_px;
    float    panel_x_left_px;
    float    panel_width_px;
    const uint8_t* command_utf8;    uint32_t command_len;
    const uint8_t* subtitle_utf8;   uint32_t subtitle_len;   // nullable
    uint8_t  agent_id;              // 0 = none; rest = enum mirroring CLIAgent
    uint32_t badge_tint_rgba;
    uint32_t header_bg_rgba;
    uint32_t command_fg_rgba;
    uint32_t subtitle_fg_rgba;
    uint32_t divider_rgba;          // 0 = no divider above this header
    uint8_t  is_sticky;             // 0/1
    float    body_clip_y_top_px;    // exclusive lower bound for body grid render
    float    body_clip_height_px;
} BtBlockHeaderEntry;

void bt_renderer_draw_block_list(
    /* existing params */
    const BtBlockLayoutEntry* bodies, uint32_t body_count,
    const BtBlockHeaderEntry* headers, uint32_t header_count,
    float scroll_offset_y_px
);
```

Sticky semantics: when a block is being stuck, Swift emits **one** `BtBlockHeaderEntry` with `is_sticky=1` and `header_y_top_px` set to the pinned **screen-space** Y. The same block's natural in-flow header descriptor is **omitted**. No double-draw, no Swift overlay.

UI font size & icon atlas calls:

```c
void bt_renderer_set_ui_font_sizes_px(float subheadline_px, float caption2_px, float scale);
// Called on init and on UITraitCollection change.
```

The icon atlas needs no FFI: agent PNGs are embedded in the Rust crate (§3.3.3).

#### 3.3.2 Draw order per frame

Mirroring Warp's `block_list_element` paint pass:

1. Background fill (`ShadcnBackground` rgba).
2. **For each body** in `bodies`: `scene.start_layer(ClipBounds::Rect(body_clip_rect))` → terminal grid cells → `scene.end_layer()`. The clip rect is set so cells cannot draw into the header band above (or, for the sticky case, into the pinned band).
3. **For each non-sticky header**, in input order: divider hairline (if `divider_rgba != 0`) → bg quad → badge circle quad → badge icon quad → command text run → subtitle text run.
4. **The sticky header last** (z-sorts on top of everything else): same draw subroutine as a regular header, but at the pinned screen-Y. No divider above the sticky band.

New Rust modules:

- `rust-core/bedterm_core/src/renderer/header_band.rs` — layout math (which glyphs fit, truncation), draw entry point.
- `rust-core/bedterm_core/src/renderer/ui_text.rs` — a second cosmic-text `Buffer` configured for proportional UI font; reuses existing glyph atlas allocator from `glyph_raster.rs` patterns.
- `rust-core/bedterm_core/src/renderer/icon_atlas.rs` — decode embedded PNGs at startup, allocate texture slots.

#### 3.3.3 UI text — cosmic-text + swash

We already use `cosmic-text = "0.12"` + `swash = "0.1.18"` for terminal cell rasterization in `glyph_raster.rs`. UI text reuses the same dependencies:

- A second `FontSystem`-backed `Buffer` configured with `Family::Name("SF Pro")`, fallback `Family::SansSerif`. On iOS the system fontdb surfaces SF.
- Two `Metrics` instances — one for subheadline, one for caption2 — sized in px from Swift's resolved Dynamic Type via `bt_renderer_set_ui_font_sizes_px`.
- Header strings are single-line, truncated with ellipsis at width. cosmic-text's `Buffer` handles shaping (kerning, CJK fallback, bidi) — we get all of that for free, no FFI rasterization bridge.
- Rasterization output is a swash bitmap, uploaded into the atlas the same way terminal glyphs are.
- Cache key is `(style, family, size_px, glyph_id)`. Atlas eviction (if it ever fills) is out of scope for v1 — header glyph set is small (mostly ASCII + a handful of CJK/Cyrillic depending on locale).

Re-rasterization triggers: trait collection change (`viewWillTransition`, Dynamic Type change) → Swift recomputes px sizes, calls `bt_renderer_set_ui_font_sizes_px`. Rust invalidates the UI-text cache; subsequent frames re-shape.

#### 3.3.4 Icon atlas — embedded PNGs

Move agent logos out of `BedTermKit/Sources/BedTermKit/Resources/CLIAgents.xcassets/` and into `rust-core/bedterm_core/assets/agent_badges/`. Current assets:

- `claude.png` (was `ClaudeLogo.imageset`)
- `codex.png` (was `OpenAILogo.imageset`)
- `generic.png` — new. Replaces the `sparkles` SF Symbol fallback for known-but-unbundled agents. Rust-controlled glyph so we ship a single PNG instead of bridging SF Symbols.

Each PNG is rendered template-style (white on transparent), included via `include_bytes!`, decoded once at first draw using `image` crate (add to `Cargo.toml`; small, no_std-compatible). All three packed into a single `MTLTexture` at slot indices stable across runs.

The badge fragment shader multiplies the template glyph by `badge_tint_rgba` from the header descriptor, so the brand colors stay defined where they are today (in `BlockHeader.swift`'s `agentTints` table — that table moves to Swift's renderer-config layer).

### 3.4 Selection migration

`BlockListSelectionController` today is constructed with `contentView` and uses it for:

- Hit-testing (`hitResolver` closure) — converts screen point to `(blockID, row, col)`.
- Coordinate base for the long-press gesture.

Changes:

- `init` signature: replace `contentView: UIView` with `offsetProvider: () -> CGFloat`.
- Long-press gesture is added to `inputView` (was `view` — same effective surface).
- `hitResolver` walks the same `computeBlockRanges` table the layout pass builds, offsetting by `offsetProvider()` instead of `contentView.convert(...)`.
- Selection highlight rendering: today the selected region tints body cells via Rust (existing path, no change). If selection visualization currently relies on any UIKit overlay (verify during implementation), it moves into the header_band draw as another quad in step 3 of the draw order.

## 4. Data flow per frame

```
displayLink tick
  ├─ if momentum live: advance contentOffsetY
  ├─ updateContentHeight (sum of header + body + gap per block)
  ├─ if scrollPosition == .followsBottom: contentOffsetY = max(0, contentHeight - viewHeight)
  ├─ computeBlockRanges → [(block, top, bot)]
  ├─ build bodies: [BtBlockLayoutEntry]    (existing — mostly unchanged)
  ├─ build headers: [BtBlockHeaderEntry]
  │     • one per visible block (intersect viewport ± overscan)
  │     • plus zero-or-one with is_sticky=1 (replaces the natural entry for that block)
  └─ bt_renderer_draw_block_list(bodies, headers, contentOffsetY)
```

Header descriptor construction in Swift pulls fields from the existing `BlockHeader.swift` value path:

- `command_utf8` — `block.command` (or localized placeholder if empty).
- `subtitle_utf8` — same `exit N · 1.2s` / `running…` builder we have today, kept in Swift.
- `agent_id`, `badge_tint_rgba` — from `CLIAgent` enum + the `agentTints` table (which moves to a `BlockHeaderModel` value type).
- `header_bg_rgba`, `command_fg_rgba`, `subtitle_fg_rgba`, `divider_rgba` — resolved from existing color tokens (`ShadcnBackground`, `ShadcnPrimary`, `ShadcnMutedForeground`, divider color) via the trait collection.

`BlockHeader.swift` (the SwiftUI view) is **deleted**. Its data accessors move to `BlockHeaderModel.swift`:

```swift
struct BlockHeaderModel {
    let command: String
    let subtitle: String?
    let agent: CLIAgent?
    static func tint(for agent: CLIAgent?) -> UInt32 { ... }   // was agentTints
}
```

## 5. Localization

Header strings sourced via `String(localized:)` in Swift — unchanged. CJK / Cyrillic / Hiragana glyphs are rasterized by cosmic-text's fontdb fallback chain on the Rust side; no bridge work needed.

Localizable copy that moves into header descriptors:

- "(no command captured)"
- "exit \(N)" / "running…"
- Duration formatting ("1.2s" / "2m 15s") — keep in Swift, send the formatted string.

## 6. File layout

**New (Rust):**

- `rust-core/bedterm_core/src/renderer/header_band.rs`
- `rust-core/bedterm_core/src/renderer/ui_text.rs`
- `rust-core/bedterm_core/src/renderer/icon_atlas.rs`
- `rust-core/bedterm_core/assets/agent_badges/{claude,codex,generic}.png`

**New (Swift):**

- `BedTermKit/Sources/BedTermKit/Features/Blocks/BlockHeaderModel.swift`
- `BedTermKit/Sources/BedTermKit/Features/Blocks/BlockListMomentum.swift` (momentum animator + sample ring)

**Modified:**

- `BlockListContainerView.swift` — gut UIScrollView/contentView/hosts; add `inputView`, `panGR`, `contentOffsetY`.
- `BlockListContainerView+Scroll.swift` — pan handler; momentum advance per tick; clamp.
- `BlockListContainerView+Sticky.swift` — keep sticky-index math; emit `BtBlockHeaderEntry` instead of mutating hosts.
- `BlockListContainerView+Layout.swift` — produces `[BtBlockHeaderEntry]` alongside bodies.
- `BlockListContainerView+Selection.swift` (or `BlockListSelectionController.swift`) — `contentView` → `offsetProvider`.
- `rust-core/bedterm_core/src/ffi/blocks.rs` — extend FFI.
- `rust-core/bedterm_core/Cargo.toml` — add `image` dep for PNG decode.

**Deleted:**

- `BedTermKit/Sources/BedTermKit/Features/Blocks/BlockHeader.swift`
- `BedTermKit/Sources/BedTermKit/Resources/CLIAgents.xcassets/ClaudeLogo.imageset`
- `BedTermKit/Sources/BedTermKit/Resources/CLIAgents.xcassets/OpenAILogo.imageset`

## 7. Transition strategy

Single PR, single cutover. The old `bt_renderer_draw_block_list` signature is replaced (not duplicated) — both sides land together. No feature flag: the change is internal to a controller, fully covered by snapshot + unit tests.

Build order during development:

1. Rust: add `BtBlockHeaderEntry`, `ui_text.rs`, `icon_atlas.rs`, draw path. Keep the Swift side calling the new FFI with an empty `headers` array — net visual change zero, validates the new FFI is wired correctly.
2. Swift: build `[BtBlockHeaderEntry]` for non-sticky headers. Remove `headerHosts` + their `UIHostingController` plumbing. Visually: per-block headers now Rust-painted.
3. Swift: port sticky math to emit the sticky descriptor. Remove `pinnedHost` + `pinnedBackground`.
4. Swift: replace `UIScrollView` with `inputView` + pan + momentum. Migrate selection controller.
5. Delete `BlockHeader.swift`, scrollView/contentView, imageset directories.

Each step ends with a working build that can be smoke-tested in the simulator before moving on.

## 8. Testing

**Unit (XCTest, `BedTermTests`):**

- `BlockListMomentumTests` — given a sample ring, expected velocity; given (v, dt), expected decayed v; clamp behavior at top/bottom edges.
- `StickyHeaderDescriptorTests` — given a list of `BlockRange` + scrollY, the sticky descriptor's `header_y_top_px` matches the existing `+Sticky.swift` output. Parametrize on the three Warp cases (fully visible, straddling top, scrolling off).
- `BlockHeaderModelTests` — tint resolution, subtitle formatting under each block state (running, exit 0, exit non-zero, no duration).

**Rust unit:**

- `header_band` tests for truncation, glyph layout pixel positions at a fixed font px.
- `icon_atlas` decode + slot allocation test.

**Snapshot / manual:**

- Run sim, exercise: scroll up through long output, scroll back down, sticky transition between adjacent blocks, momentum fling, long-press selection. Compare against current `main` visually (acceptable diffs: pixel-perfect text positions may shift slightly because cosmic-text now shapes header text instead of SwiftUI; brand visuals unchanged).

CI: `cargo fmt --check`, `cargo clippy -D warnings`, `xcodebuild test`, `mint run swiftlint lint --strict`, `xcrun swift-format lint -r --strict`.

## 9. Accepted v1 losses

- Scroll-to-top status-bar tap.
- VoiceOver focus per individual header (descriptors will support synthesizing accessibility elements as a follow-up).
- Dynamic Type beyond what re-rasterization on trait change supports (cheap; should be fine — calling out the dependency).
- `sparkles` SF Symbol fallback replaced by a `generic.png` Rust-side asset.

## 10. Open questions

None blocking.
