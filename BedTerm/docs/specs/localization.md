# Localization — iOS Implementation Spec

PRD: [`prd/bedterm/localization.xml`](../../../prd/bedterm/localization.xml)

## Architecture

String Catalogs (`.xcstrings`, iOS 17+) consolidated in the **app target**, populated from both the app and the `BedTermKit` SwiftPM package. The English literal is the key — Xcode shows per-locale state (`translated`, `needs_review`, `stale`) in the catalog UI.

- `BedTerm/Localizable.xcstrings` — every chrome string. Lookups from SwiftUI `Text("…")` and from `String(localized: "…")` resolve against `Bundle.main`, so strings inside the package find this catalog without a `Bundle.module` annotation.
- `BedTerm/InfoPlist.xcstrings` — `NSLocalNetworkUsageDescription` and any future `NS…UsageDescription` keys. The English value still lives in `Info.plist` (Xcode treats it as the development-region default); translations come from this catalog.

Six locales declared in `knownRegions`: `Base`, `en`, `zh-Hans`, `zh-Hant`, `ja`, `ko`, `ru`. `CFBundleAllowMixedLocalizations` and `UIPrefersShowingLanguageSettings` are set so Settings → BedTerm → Language exposes the per-app override.

## Authoring rules for new strings

- **SwiftUI literals** — write English directly: `Text("Connect")`, `Button("Cancel")`, `.navigationTitle("…")`, `TextField("Host", …)`, `Label`, alert titles, `.accessibilityLabel("…")`. Catalog auto-extracts these on build.
- **Non-SwiftUI strings** — wrap in `String(localized: "…")` at the point of construction (view models, helpers, anything that builds an error message). Use interpolation `\(value)` for substituted values; keep the surrounding sentence in the literal.
- **Never** concatenate localizable text with `+` — that downgrades the expression from `LocalizedStringKey` to `String` and the key isn't extracted. If a sentence is long, keep it on one line.
- **Never** use Swift string comparison on a user-facing string for control flow. Surface a typed error or enum case instead (see `TerminalSession.lastError`).
- **Don't wrap**: terminal pty output, debug log lines, assert messages, the brand name "BedTerm" — per PRD R4.

## Routing typed errors

`TerminalSession.lastError: SSHError?` is the source of truth for the host-key-mismatch branch in `ConnectionViewModel`. Previously the view model parsed `"Host key changed."` out of the reason string — that broke the moment the reason was localized. New code that needs to make decisions based on an SSH failure should match on `SSHError`, not on the displayed reason string.

## Translation workflow

Translations are produced by the engineering agent in-session. Editing the catalog JSON directly is the canonical workflow (Xcode's catalog UI works too):

1. Add the new English literal (SwiftUI auto-extract or `String(localized:)`).
2. Build once so Xcode adds the entry to `Localizable.xcstrings` with `extractionState: "extracted_with_value"` and `state: "new"` on every non-source locale.
3. Fill `zh-Hans`, `zh-Hant`, `ja`, `ko`, `ru` values; set `state: "translated"` on each.
4. When changing an existing English literal, Xcode marks other locales `stale` automatically — refresh those before merging.

## Translation quality rules (PRD R6)

- Natural, contemporary developer-tool register — neither overly formal nor literal.
- Native technical-term conventions (SSH, host, port, password, passphrase, fingerprint) follow each language's developer community usage.
- CJK uses full-width punctuation (`，`, `。`, `「」`); Korean follows the standard spacing rules; Russian punctuation is native.
- Russian and other typically-longer translations either fit, wrap cleanly, or are abbreviated by the translator into a shorter native phrasing.

## Verification

Manual: build the app, set the scheme's "App Language" to each of the six locales, and walk Onboarding → Connection form → (induced) error → HostKeyMismatch screen. The Lookin debug inspector (R12 of the MVP PRD) is helpful for confirming each Text view's resolved string at runtime without re-launching.

Automated: the existing `TerminalSessionDescribeTests` suite verifies each `SSHError` case still maps to a non-empty user-facing string after the `String(localized:)` wrapping. No new test infrastructure for localization in this iteration — snapshot tests across locales are deferred.

## Sub-tasks

| Status | Sub-task | Notes |
|---|---|---|
| ✅ | Audit user-facing chrome strings | ~70 strings across Connection, Onboarding, HostKeyMismatch, Terminal |
| ✅ | English copy polish | Removed placeholder tutorial paragraph, disambiguated closed-reason strings, renamed "Reject and go back" → "Don't connect", nav title → "New Connection" |
| ✅ | Refactor `TerminalSession` to expose `lastError: SSHError?` | Removes English-prefix string matching from `ConnectionViewModel` |
| ✅ | Wrap non-SwiftUI strings in `String(localized:)` | View-model error messages, computed Onboarding text, terminal-session reasons |
| ✅ | Create `Localizable.xcstrings` with English + 5 translations | App-target consolidation; ~70 keys |
| ✅ | Create `InfoPlist.xcstrings` with English + 5 translations | `NSLocalNetworkUsageDescription` |
| ✅ | Wire pbxproj: file refs, Resources phase, `knownRegions` | Six locales |
| ✅ | Add `CFBundleAllowMixedLocalizations` + `UIPrefersShowingLanguageSettings` | Enables per-app language override under Settings |
| ✅ | Build + run test suite with all six locales declared | Build succeeded, 8 test suites pass |
| ❌ | Manual on-simulator walkthrough per locale | Pending — user-driven |
| ❌ | Add a SwiftLint custom rule banning unwrapped user-facing literals outside SwiftUI views | Deferred — out of scope for this iteration |
