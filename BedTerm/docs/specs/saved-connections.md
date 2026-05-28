# Hosts — iOS Implementation Spec

PRD: `prd/bedterm/saved-connections.xml`. This document describes **how** the Hosts feature is built on iOS. Behavior lives in the PRD.

## Product posture

BedTerm is a free, no-ad developer tool for people who are comfortable around terminals. The host manager should feel like a sharp instrument: modern, native, and tactile, but never promotional or ornamental. The selected visual direction is **functional glass with restraint**:

- Host rows stay quiet and high-contrast so `user@host:port`, labels, auth badges, and errors remain easy to scan.
- Liquid Glass is used as a functional control layer for navigation, actions, recovery controls, active progress, and confirmation surfaces.
- Motion is stateful feedback, not decoration.
- Native SwiftUI/iOS controls are preferred over custom-drawn controls whenever they satisfy the interaction.

## Architecture

A new top-level Feature module + a new Core storage type, plus a small split of the existing `ConnectionViewModel` so the connect path can be reused from two screens.

- **App / Routing.** `AppRoute` gains `.hosts` and `.hostForm(SavedHost?)`. `.terminal` and `.hostKeyMismatch` are unchanged. Onboarding's `onFinish` lands on `.hosts` rather than the Connection form.
- **Features / Hosts** (`BedTermKit/Sources/BedTermKit/Features/Hosts/`).
  - **UI is Rust.** `rust-core/bedterm-ios/src/hosts/hosts_vc.rs` (`BtIosHostsListViewController`) renders the list: rounded `make_list_row` cells, empty-state label, navbar `+` button, left-swipe delete. Reads entries via `bt_swift_hosts_snapshot_json` (see `HostsBridge.swift`) and drives connect / delete via the same bridge.
  - `HostsConnectController.swift` — UIKit controller that owns the Rust list VC + `HostsViewModel`, drives the swap-confirm dance, delete confirmation, host-key-mismatch sheet, and toaster via `withObservationTracking`.
  - `HostsBridge.swift` — `@_cdecl` shims (`bt_swift_hosts_snapshot_json`, `_delete`, `_connect`, …) that route the Rust VC's row taps back into `HostsViewModel`.
  - `HostsViewModel.swift` — `@Observable`. Owns `entries: [SavedHost]`, `inFlightID: UUID?`, `errorByID: [UUID: RowError]`, `pendingMismatch: PendingMismatch?`. Exposes `requestConnect(id:)` (called from the bridge — performs the swap-confirm dance), plus `add`/`edit`/`delete` and a `connect(id:)` driver that delegates the SSH path to a shared `ConnectAttempt`.
- **Features / Connection** — the add/edit form.
  - **UI is Rust.** `rust-core/bedterm-ios/src/connect_form/connect_form_vc.rs` (`BtIosConnectFormViewController`) renders the two-card Shadcn form (Connection + Authentication). Takes `editing_id_or_null` for `.add | .edit(SavedHost)` mode and `connect_on_save: bool`; the latter flips the Save button title to "Save & Connect".
  - `ConnectFormBridge.swift` — `@_cdecl` shims (`bt_swift_connect_form_prefill_json`, `_save`, `_pick_key`, …) that route the Rust VC's form submission into `ConnectionFormViewModel.save(...)`, including the `UIDocumentPickerViewController` private-key import.
  - The Rust VC reports the outcome via a C callback that the Swift coordinator converts to `ConnectionFormOutcome.saved(id)` / `.savedAndConnect(id)` / `.cancelled`. `HostsConnectController` refreshes the list and, on `.savedAndConnect`, calls `HostsViewModel.connect(id:)` once the navigation pops back.
  - `ConnectionFormViewModel.swift` (extracted from `ConnectionViewModel`) — validation + save. No prewarm, no SSH.
  - `ConnectAttempt.swift` — pulled out of today's `ConnectionViewModel`. A small `@MainActor` driver that takes a `HostCredential`, runs the prewarm, opens `TerminalSession`, and reports `(session, pendingMismatch, errorMessage)`. Reused by `HostsViewModel`.
- **Core / Keychain.**
  - `HostsStore.swift` — per-entry Keychain item (`service = "com.applovin.yi.bedterm.savedHosts"`, `account = id.uuidString`), plus a `UserDefaults`-backed ordered index (`"hosts.order"` → `[UUID]`).
  - `CredentialsStore.swift` — kept temporarily only to read the legacy single-credential entry during migration; new writes go to `HostsStore`.
- **Onboarding.** No behaviour change other than the `onFinish` destination.

## Key types

| Type | Role |
|---|---|
| `SavedHost` | `Codable, Equatable, Identifiable`. `id: UUID`, `label: String` (may be empty — UI defaults to `user@host`), `credential: HostCredential`. |
| `HostsStore` | `list() -> [SavedHost]`, `load(id:) -> SavedHost`, `save(_ entry: SavedHost) throws`, `delete(id:) throws`, `migrateLegacyIfNeeded()`. Holds the ordered index in `UserDefaults`. |
| `ConnectAttempt` | `@MainActor`. `run(credential:) async -> Outcome`. `Outcome = .session(TerminalSession) / .mismatch(PendingMismatch) / .error(String, permissionDenied: Bool)`. Single owner of prewarm + `TerminalSession.connect` so both screens share one code path. |
| `HostsViewModel` | `@Observable`. Loads + reorders the list, owns per-row state (`inFlightID`, `errorByID`), and exposes `add`, `edit`, `delete`, `connect(id:)`. |
| `ConnectionFormViewModel` | `@Observable`. Owns the form fields + validation + `save() throws -> SavedHost.ID`. No connect logic. |

## Storage layout

Keychain items:

| Service | Account | Value |
|---|---|---|
| `com.applovin.yi.bedterm.savedHosts` | `<entry-uuid>` | JSON-encoded `SavedHost` |
| `com.applovin.yi.bedterm.credentials` (legacy) | `default` | Existing single `HostCredential`. Read at migration, then deleted. |

`UserDefaults` keys:

| Key | Value |
|---|---|
| `hosts.order` | `[String]` of UUID strings, in display order. |

Reading the list is `order.compactMap { try? store.load(id: $0) }`; missing UUIDs are filtered (defensive against partial corruption).

## Connect path

`HostsViewModel.connect(id:)`:

1. Sets `inFlightID = id`, clears the row's prior error.
2. `let entry = try store.load(id: id)`.
3. `let outcome = await connectAttempt.run(credential: entry.credential)`.
4. On `.session(session)` → publishes `lastSession`, the screen pushes `.terminal`.
5. On `.mismatch(pending)` → screen pushes `.hostKeyMismatch(...)`; trusting the new key calls `connect(id:)` again.
6. On `.error(msg, permissionDenied: pd)` → writes into `errorByID[id]`; the row renders the same `Open Settings / Retry` block today's screen does (re-usable view).
7. `defer { inFlightID = nil }`.

The `errorByID` map is keyed by entry id so only the affected row shows the error; tapping a different row clears the prior row's banner.

## iOS 26 visual system

Use the system's iOS 26 Liquid Glass APIs as a functional layer. Do not blur every row or make the host list look like a stack of decorative cards; the list is content, and content needs to remain stable and readable.

Targets for glass treatment:

- Top-trailing Add button and any compact toolbar action that floats above content.
- `Save`, `Cancel`, `Retry`, and `Open Settings` controls where they appear in transient surfaces.
- Expanded row recovery controls after a connection failure.
- Confirmation surfaces and form presentation chrome when the system component does not already provide the current platform appearance.
- Active connecting affordances if they are drawn as custom chips or row accessories.

Implementation notes:

- Prefer native SwiftUI components first. On iOS 26+, `ToolbarItem`, `Button`, `List`, `Form`, `.sheet`, `confirmationDialog`, `Menu`, `Picker`, and `ProgressView` should receive the current system appearance automatically when used idiomatically.
- For custom controls that need explicit glass, gate with `#available(iOS 26, *)` and use `.glassEffect(_:in:)` after layout and appearance modifiers.
- Wrap grouped custom glass controls in `GlassEffectContainer` so the system can render them efficiently and morph related shapes together.
- Use `.interactive()` only for tappable, focusable, or otherwise interactive elements. Static labels, host row text, and passive badges should not use interactive glass.
- Use `.buttonStyle(.glass)` for secondary glass actions and `.buttonStyle(.glassProminent)` for the primary action where that style is available and matches the hierarchy.
- Provide fallback styling for earlier iOS versions using standard system materials or ordinary bordered/prominent button styles. Fallbacks must be readable with reduced transparency and increased contrast enabled.

Custom row guidance:

- Host rows use `List` identity and system row selection behavior. Keep the row content background quiet; if a row needs a custom background for grouping, use semantic asset colors or standard materials rather than explicit glass.
- Auth badges are compact labels, not primary controls. They use semantic color tokens and accessibility labels; they do not need Liquid Glass.
- The expanded error recovery area may use a glass container on iOS 26 because it is a transient control cluster. Its message remains plain, high-contrast text.

## Motion and transitions

Motion should help people understand state changes:

- Add/edit form presentation uses system sheet/navigation transitions.
- Save and cancel dismiss using the system transition for the current presentation.
- Row insertion and deletion use the standard `List` insertion/removal animation.
- Connecting state crossfades the row subtitle to `Connecting...`, reveals `ProgressView`, and dims/disables only the tapped row.
- Switching from collapsed error to expanded error animates height and control appearance with a short, spring-like system animation.
- Retrying from an error morphs the recovery controls back into the connecting affordance when implemented with glass IDs on iOS 26; otherwise it uses a normal opacity/height transition.

Respect accessibility:

- If `accessibilityReduceMotion` is enabled, avoid springy or morphing transitions; use simple opacity changes or system default transitions.
- If transparency is reduced or contrast is increased, prefer nontransparent system backgrounds and bordered/prominent buttons over custom glass.
- Dynamic Type may increase row height. Row layout must wrap or truncate host metadata predictably without overlapping badges or controls.

## Migration

`HostsStore.migrateLegacyIfNeeded()` runs once in `HostsViewModel.init`:

1. If `UserDefaults.hosts.order` is non-empty, no-op.
2. Else attempt `CredentialsStore().load()`. On success, create a `SavedHost(id: UUID(), label: "Last connection", credential: legacy)`, persist it, append to the index, and call `CredentialsStore().delete()`.
3. Errors are swallowed (logged via `os.Logger`), since a failed migration must not block the user from seeing an empty list and adding a new entry.

## Localization

New user-facing strings (all auto-extracted from SwiftUI literals or wrapped in `String(localized:)`):

- `"Hosts"` — nav title.
- `"Add Host"` — empty-state primary action + `+` accessibility label.
- `"No hosts yet"` — empty-state headline.
- `"Add a server to connect from your bed."` — empty-state body.
- `"New Host"` / `"Edit Host"` — form nav titles.
- `"Label (optional)"` — form label field placeholder.
- `"Save"`, `"Cancel"`, `"Delete"`, `"Edit"` — toolbar / context actions.
- `"Delete \"\(label)\"? This will remove the saved password or key."` — delete confirmation (interpolated value only).
- `"Last connection"` — migration label.
- `"Couldn't load saved hosts — device locked. Retry."` — locked-keychain banner.

Per `CLAUDE.md`, after edits to existing English strings (none here — this is all new copy), mark non-English entries `needs_review`.

## Design tokens

Per `CLAUDE.md` (Colors & appearance), every user-visible colour on the new surfaces comes from a named token in `Assets.xcassets` with both Light and Dark variants — no literal `Color(red:…)`, no system constants. Tokens this feature needs:

- `surface.list.background`, `surface.list.row` — list scroll background and row fill (reuse if already defined for the existing Connection form).
- `surface.transient.background` — expanded inline error/recovery area fallback on pre-iOS 26 or when transparency is reduced.
- `text.primary`, `text.muted` — row primary line and "user@host:port" subtitle.
- `badge.password.foreground` / `.background`, `badge.key.foreground` / `.background` — the auth-method badge in two flavours. Tokens, not raw palette names.
- `warning.foreground` / `.background` — duplicate-host warning and locked-device banner fallback.
- `error.foreground` — inline row error (reuse the existing token if one exists; otherwise add).

If a needed token isn't in the catalogue yet, add it before wiring the view.

## Testing

Unit tests in `BedTermTests/Hosts/`:

- `HostsStoreTests`
  - Save → list → load round-trip preserves all fields, including `privateKey` Data and passphrase.
  - Delete removes both index entry and Keychain item.
  - Migration creates exactly one entry from a populated legacy store, is idempotent on re-run, and is a no-op on a fresh install.
  - List order is preserved across reorders of the index array (set up via direct `UserDefaults` writes).
- `HostsViewModelTests`
  - `add` / `edit` / `delete` reflect into `entries` and into a stub store.
  - `connect(id:)` flows through an injected `ConnectAttempt` stub: success → `lastSession` set; mismatch → `pendingMismatch` set; error → `errorByID[id]` populated; permission-denied flag propagates.
- `ConnectionFormViewModelTests`
  - Validation: missing host / port / username surface today's localized strings.
  - Save in `.add` mode appends a new entry; in `.edit` mode mutates the existing entry's fields without changing its `id`.
  - Empty label is preserved as empty (UI handles the default rendering).
- `HostsVisualStateTests` or snapshot/UI coverage
  - Row labels, subtitles, and badges remain visible at large Dynamic Type sizes.
  - Reduced motion disables custom morphing/height spring animations.
  - Reduced transparency/increased contrast fallback uses opaque or standard system surfaces while keeping primary actions identifiable.

Keychain is wrapped behind a small protocol seam in `Keychain.swift` so the store can be exercised against an in-memory dictionary in tests. The seam stays internal — the production `enum Keychain` keeps its current API.

## Out of scope (v1)

- Reordering / drag-to-rank / multi-delete.
- Folders, tags, colours.
- iCloud sync (kept off — secrets are `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`).
- Per-entry startup commands, port forwards, environment, jump hosts.
- Auto-sort by recency / usage counts.
- Import / export / share.
- Autofill from the system password manager.
