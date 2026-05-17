# Metal Terminal Appearance Pipeline

> Linked from `prd/bedterm/design.xml` R3 (System Appearance).

## Goal

The Metal terminal renderer responds to iOS system appearance changes:
background, default foreground, and the ANSI 16-colour palette swap when the
user toggles Light ↔ Dark. The palette is sourced from the design-system token
catalogue alongside the rest of the app's colours — no literal hex anywhere in
code.

## Pipeline

```
UIUserInterfaceStyle (UIKit trait)
    │
    │  TerminalMetalUIView.applyAppearance()      ← called at init + on trait change
    ▼
TerminalPalette.resolve(for: traitCollection)     ← 18 colorsets → 18 RGB triples
    │
    ├── TerminalCore.setPalette(...)              ← Swift wrapper
    │     │
    │     └── bt_term_set_palette(handle, &BtPaletteView)   ← C ABI
    │           │
    │           └── Terminal.set_palette(Palette)            ← Rust core
    │                 │
    │                 └── used by color_to_rgba(...) at snapshot time
    │
    └── RendererBridge.setClearColor(...)
          │
          └── bt_renderer_set_clear_color(...)               ← C ABI
                │
                └── Renderer.clear_color                     ← applied in draw()
```

## Token catalogue

`BedTermKit/Sources/BedTermKit/Resources/Tokens.xcassets/` holds:

- `TerminalForeground.colorset` — default text colour (Light: near-black, Dark: light-grey).
- `TerminalBackground.colorset` — terminal canvas (Light: white, Dark: black).
- `TerminalAnsi0.colorset` … `TerminalAnsi15.colorset` — 16 ANSI indices.
  - 0–7 are the standard ANSI block; 8–15 are the "bright" block.
  - Light-mode values track Apple Terminal's *Basic Light* theme (darker,
    higher-contrast variants for slots that would otherwise wash out on white).
  - Dark-mode values track the legacy renderer defaults so existing Dark users
    see no visible change.

Adding new palette tokens or recolouring existing ones happens in this folder
only — `TerminalPalette.swift` just reads them.

## Rust-side palette default

`Palette::default()` returns the legacy hardcoded set used before this feature
landed, so:
- Rust unit tests that don't push a palette render exactly as before.
- The very first frame on launch (before the first `applyAppearance()` call
  completes) does not flash a different background — it stays on the legacy
  defaults, which were Dark.

## Why the renderer needs its own clear-colour FFI

The Metal pass clears the framebuffer before drawing any cells. If only the
per-cell `bg_rgba` reflected the palette but `MTLLoadAction::Clear` stayed
black, any region not covered by a cell quad (e.g. rounded corners, viewport
padding) would still show through black. `bt_renderer_set_clear_color` lets the
host keep the clear value in sync with the palette's `default_bg`.

## Live trait switching

`TerminalMetalUIView` registers two trait observers via the iOS 17+
`registerForTraitChanges` API:
1. `UITraitPreferredContentSizeCategory` — re-rasterises the glyph atlas for
   Dynamic Type (pre-existing behaviour).
2. `UITraitUserInterfaceStyle` — calls `applyAppearance()` whenever the
   resolved style changes.

The Light/Dark observer guards against spurious callbacks where the style
didn't actually change (`prev.userInterfaceStyle !=
self.traitCollection.userInterfaceStyle`) to avoid redundant work.
