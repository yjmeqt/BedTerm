# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build, test, lint

Bootstrap once per checkout:

```sh
brew install mint
mint bootstrap
rustup show   # materialises the toolchain pinned by rust-core/rust-toolchain.toml
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin
```

The Rust core (`rust-core/bedterm-core`) is packaged into `BedTermKit/BinaryFrameworks/BedTermIOS.xcframework` (consumed as a SwiftPM `.binaryTarget`). The `BedTerm.xcscheme` build pre-action runs `scripts/build-rust-xcframework.sh ${CONFIGURATION} ${PLATFORM_NAME}` automatically — `PLATFORM_NAME` narrows the build to the one slice Xcode actually needs (`iphonesimulator` → sim only; empty/release → all three). The script is idempotent: a no-op build skips the `-create-xcframework` step entirely. The whole xcframework directory is gitignored — every build regenerates `Info.plist` plus the slice `.a` files from scratch.

Build & test go through the `xc-dev` skill (`xc-dev build|test|run` — tasks defined in `.xc-dev/tasks.toml`); the skill picks the right simulator (via `.xc-dev/simulator.toml`, with the per-machine UDID cached in `simulator.local.toml`) and the Rust xcframework pre-action runs from the `BedTerm.xcscheme`. Tests live inside the `BedTermKit` Swift package (`BedTermKit/Tests/BedTermKitTests/`) and use the Swift Testing framework (`import Testing`, `@Suite`, `@Test`, `#expect`, `#require`). New test files don't need any Xcode project bookkeeping — SwiftPM picks them up automatically.

Lint (must pass):

```sh
xcrun swift-format lint -r --strict BedTerm BedTermKit/Sources BedTermKit/Tests
```

`swift-format` ships with Xcode 26 — no install needed.

For iOS workflows (simulator boot, run, log streaming), prefer the `xc-dev` skill; reserve `xcodebuildmcp-cli` for UI automation and debugging.

## Branching model

Worktree-per-feature. Always:

```sh
git worktree add .worktrees/<feature> -b feature/<feature>
```

Work happens in the worktree; commit and PR back to `main`. Don't use `EnterWorktree`.

## Localization (i18n)

The app ships in English (base), Simplified Chinese, Traditional Chinese, Japanese, Korean, and Russian. See `prd/bedterm/localization.xml` for the authoritative rules. Every user-facing string written from now on **must** be localizable:

- **SwiftUI literals** — `Text("Connect")`, `Button("Cancel")`, `.navigationTitle("Setup")`, `TextField("Host", …)`, `Label("…", systemImage: …)`, alert titles/messages, and `.accessibilityLabel("…")` are auto-extracted into `Localizable.xcstrings`. Write them as plain English string literals — do not concatenate or interpolate user-visible copy at the call site.
- **Non-SwiftUI strings** — anything passed to `Alert`, thrown as an error message, surfaced from a view model, or built in a helper must be wrapped: `String(localized: "Host, port and username are required.")`. Use string interpolation only for substituted values (`String(localized: "Connect to \(host)")`), never for the surrounding sentence.
- **Info.plist usage descriptions** — translations live in `InfoPlist.xcstrings`. When adding a new `NS…UsageDescription`, add the English entry to `InfoPlist.xcstrings`, not directly to `Info.plist`.
- **Never-localized** — terminal pty bytes (rendered verbatim from the remote shell), the brand name "BedTerm", and developer-only strings (debug logs, asserts, crash messages). Do not wrap those.

When you change English copy, mark the affected non-English entries as `needs_review` in the catalogue so the translator pass picks them up. When you add a new string, the catalogue gains the new key on next build — fill the other locales before merging.

## Colors & appearance

The app follows the iOS system appearance (R9). Every colour the user sees — surface, text, icon, border, accent, error, keybar background/label, disconnect banner, etc. — **must** come from the design system as a named colour set in `Assets.xcassets` with both `Any Appearance` (Light) and `Dark Appearance` variants. Reference them via `Color("TokenName")` / `UIColor(named: "TokenName")`.

- **Never** write literal colours in code or views: no `Color(red:green:blue:)`, no `UIColor(red:green:blue:)`, no hex strings, no `Color.black` / `.white` / `.gray` / other `Color.<name>` system constants on user-visible surfaces.
- Use **semantic** token names (`surface.primary`, `text.muted`, `keybar.background`, `accent`, `error`) — not raw palette names (`gray800`, `blue500`).
- Symbolic SwiftUI colours that are already adaptive (`Color.primary`, `Color.secondary`, `.tint`, `.accentColor`) are acceptable when a token isn't needed, but prefer a named token for anything brand- or component-specific.

If you find yourself reaching for a hex value, stop and add the token to the catalogue first.

## PRDs

Product requirements live in `prd/<module>/<feature>.xml` (pure XML, schema documented in the `/prd` skill). The `prd` CLI (`uv tool install git+https://github.com/yjmeqt/prd-tool.git`) validates, formats, and rolls up stats:

```sh
prd validate prd/<module>/<feature>.xml
prd format prd/<module>/<feature>.xml
prd stats prd/index.xml
```

Use the `/prd <module>/<feature>` skill to load a PRD with its Figma context before starting work on the feature it describes. Index lives at `prd/index.xml`; currently the only module is `bedterm`.

PRDs describe **what** the product does (rules, bugs, Figma refs) — never type names, hex codes, pixel values, or file paths. Implementation specs live in `bedterm/docs/specs/<feature>.md` and are linked from `<implementation spec="…">` in the PRD.

After implementing a rule, set its `status="✅"` and run `prd format` to normalise. Never rename or reuse a `rule id` or `bug id` — bugs and conversation history reference them permanently. Bugs move `Open → Fix Pending → Fixed`; only the user marks `Fixed`.

## Testing methodology

All visual changes to the terminal renderer (font, layout, palette, block headers, cell grid)
MUST be verified offline via the CLI tools **before** touching the iOS simulator. The CLI toolchain
is the fast path — no simulator boot, no Xcode build, instant PNG output.

### Unit convention

All spatial values in CLI flags are **points** (UIScreen.bounds logic units).
Device pixels = points × scale.

```
--device iphone17          # 402×874 pt @ 3x → 1206×2622 px PNG
--font-size 14             # font size in points (matches UIFont.pointSize)
--scale 3.0                # device-pixel ratio (matches UIScreen.scale)
```

### Device presets

Use `--device <name>` to pick a real iOS screen. Scale and viewport are set automatically.
Full list in `rust-core/bedterm-core/src/device_presets.rs`.

| Flag | Viewport (pt) | Scale |
|---|---|---|
| `--device iphone17` | 402×874 | @3x |
| `--device iphone17-promax` | 440×956 | @3x |
| `--device iphone16` | 393×852 | @3x |
| `--device iphone15-promax` | 430×932 | @3x |
| `--device iphone14` | 390×844 | @3x |
| `--device iphone-se3` | 375×667 | @2x |
| `--device ipad-pro13` | 1032×1376 | @2x |
| `--device ipad-pro11` | 834×1210 | @2x |
| `--device ipad-air13` | 1024×1366 | @2x |
| `--device ipad-air11` | 820×1180 | @2x |
| `--device ipad-mini` | 744×1133 | @2x |
| `--device mac` | 1200×800 | @2x |

### Palette

Two presets matching the iOS app's `Tokens.xcassets` colour catalogue:

```sh
--palette bedterm-dark     # default: fg=#CCC, bg=#000, ANSI per asset catalog dark appearance
--palette bedterm-light    # fg=#1A1A1A, bg=#FFF, ANSI per asset catalog light appearance
```

Header chrome (command text, subtitle, divider, band fill) derives from the active palette
automatically — no per-palette hardcoded tokens needed.

### Query cell metrics

```sh
# Font size in pts, scale = device-pixel ratio
cargo run -p bedterm-core --bin bedterm-render cell-size --font-size 14 --scale 3.0
# → 35 50   (cell_w_px cell_h_px)
```

Derive cols/rows: `cols = viewport_px.w / cell_w_px`, `rows = viewport_px.h / cell_h_px`.

### Rust renderer changes → offline CLI (macOS)

No simulator needed. Build and render a fixture, compare visually or against golden PNG:

```sh
# Grid mode — terminal frame from stdin byte stream
echo -e '\x1b[32mHello World\x1b[0m' | cargo run -p bedterm-core --bin bedterm-render grid > /tmp/out.png

# Grid mode with device preset
cargo run -p bedterm-core --bin bedterm-render grid --device iphone17 > /tmp/out.png

# Block list mode — wrap stdin as a single block
ls --color=always | cargo run -p bedterm-core --bin bedterm-render blocks --wrap > /tmp/out.png

# Block list with device + palette
cat multi-block.bin | cargo run -p bedterm-core --bin bedterm-render blocks \
    --device iphone17 --palette bedterm-light > /tmp/out.png

# From fixture files
cargo run -p bedterm-core --bin bedterm-render grid tests/fixtures/ls-color.bin > /tmp/out.png
cargo run -p bedterm-core --bin bedterm-render blocks tests/fixtures/multi-block.bin > /tmp/out.png

# Batch render across all device presets
bash scripts/sgr-test/render-all-matrix.sh
open /tmp/renders/*.png
```

### Record → render pipeline (faithful replay)

`bedterm-record` captures a real PTY session with shell integration at exact terminal
dimensions. It writes a `.meta.json` sidecar so `bedterm-render` replays at the same
cols/rows — text wrapping is byte-identical.

```sh
# Build both tools
cargo build -p bedterm-core -p bedterm-record

# Record a command
cargo run -p bedterm-record -- --cmd "ls --color=always" --device iphone17 -o session.bin

# Render with context sidecar (guarantees consistent cols/rows)
cat session.bin | cargo run -p bedterm-core --bin bedterm-render blocks \
    --context session.meta.json > out.png

# Record an interactive Claude session
bash scripts/sgr-test/record-claude-session.sh "fix the auth bug"
```

The script queries cell metrics, derives exact cols/rows, passes `--cols`/`--rows` to the
record tool, and renders with the sidecar. Both record (PTY) and render (Terminal) use
the same dimensions.

### Context override priority

1. Explicit CLI flags (`--cols`, `--rows`, `--scale`, `--viewport`, `--palette`)
2. Context file (`--context session.meta.json`)
3. Device preset (`--device iphone17`)
4. Defaults (mac viewport, scale=2.0, bedterm-dark palette)

### Swift UIKit changes → iOS only

Must run on iOS simulator or device:

```sh
xc-dev test
xc-dev run  # manual verification
```

### Fixture conventions

- `tests/fixtures/*.bin` — raw byte streams (may include OSC 133) for regression input
- `tests/fixtures/*.json` — canned session data for block list fixture tests
- `tests/fixtures/*.png` — golden output images for comparison
