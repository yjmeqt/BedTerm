# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build, test, lint

Bootstrap once per checkout:

```sh
brew install mint
mint bootstrap
```

Build & test (iOS 26 simulator):

```sh
xcodebuild test \
  -project BedTerm.xcodeproj \
  -scheme BedTerm \
  -destination 'platform=iOS Simulator,name=iPhone 17' \
  | mint run xcbeautify
```

Run a single test:

```sh
xcodebuild test \
  -project BedTerm.xcodeproj -scheme BedTerm \
  -destination 'platform=iOS Simulator,name=iPhone 17' \
  -only-testing:BedTermTests/<ClassName>/<testMethod> \
  | mint run xcbeautify
```

Lint (both must pass):

```sh
mint run swiftlint lint --strict
xcrun swift-format lint -r --strict BedTerm BedTermTests
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
