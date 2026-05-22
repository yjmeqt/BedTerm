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

The Rust core (`rust-core/bedterm_core`) is packaged into `BedTermKit/BinaryFrameworks/BedTermCore.xcframework` (consumed as a SwiftPM `.binaryTarget`). The `BedTerm.xcscheme` build pre-action runs `scripts/build-rust-xcframework.sh ${CONFIGURATION}` automatically, so any `xcodebuild build|test` rebuilds the xcframework if Rust changed. The script is idempotent — a no-op build skips the `-create-xcframework` step entirely. Slice `.a` files are gitignored; only `Info.plist` is tracked.

Build & test go through the `worktree-ios-dev` skill (`worktree-ios-dev-tool build|test|run`); the skill picks the right simulator, wires the Rust xcframework pre-action, and pipes through `xcbeautify`. Tests live inside the `BedTermKit` Swift package (`BedTermKit/Tests/BedTermKitTests/`) and use the Swift Testing framework (`import Testing`, `@Suite`, `@Test`, `#expect`, `#require`). New test files don't need any Xcode project bookkeeping — SwiftPM picks them up automatically.

Lint (both must pass):

```sh
mint run swiftlint lint --strict
xcrun swift-format lint -r --strict BedTerm BedTermKit/Sources BedTermKit/Tests
```

`swift-format` ships with Xcode 26 — no install needed. SwiftLint also runs as a SwiftPM build-tool plugin on `BedTermKit` (configured in `BedTermKit/Package.swift`); `xcodebuild` is invoked with package-plugin validation skipped (see commit `58642e8`).

For iOS workflows (simulator boot, run, log streaming), prefer the `worktree-ios-dev` skill; reserve `xcodebuildmcp-cli` for UI automation and debugging.

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
