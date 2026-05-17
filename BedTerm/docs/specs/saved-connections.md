# Saved Connections — iOS Implementation Spec

PRD: `prd/bedterm/saved-connections.xml`. This document describes **how** the saved-connections feature is built on iOS. Behavior lives in the PRD.

## Architecture

A new top-level Feature module + a new Core storage type, plus a small split of the existing `ConnectionViewModel` so the connect path can be reused from two screens.

- **App / Routing.** `AppRoute` gains `.savedConnections` and `.connectionForm(SavedConnection?)`. `.terminal` and `.hostKeyMismatch` are unchanged. Onboarding's `onFinish` lands on `.savedConnections` rather than the Connection form.
- **Features / SavedConnections** (`BedTermKit/Sources/BedTermKit/Features/SavedConnections/`).
  - `SavedConnectionsScreen.swift` — SwiftUI `List` + empty state + `+` toolbar button + swipe / context-menu actions.
  - `SavedConnectionsViewModel.swift` — `@Observable`. Owns `entries: [SavedConnection]`, `inFlightID: UUID?`, `errorByID: [UUID: String]`, `pendingMismatch: PendingMismatch?`, the `Add` / `Edit` / `Delete` calls, and a `connect(id:)` driver that delegates the SSH path to a shared `ConnectAttempt`.
- **Features / Connection** (renamed). The existing screen becomes the add/edit form.
  - `ConnectionFormScreen.swift` (renamed from `ConnectionScreen`) — takes `mode: .add | .edit(SavedConnection)`. Replaces the bottom "Connect" button with a `Save` toolbar item (and Cancel).
  - `ConnectionFormViewModel.swift` (extracted from `ConnectionViewModel`) — validation + save. No prewarm, no SSH.
  - `ConnectAttempt.swift` — pulled out of today's `ConnectionViewModel`. A small `@MainActor` driver that takes a `HostCredential`, runs the prewarm, opens `TerminalSession`, and reports `(session, pendingMismatch, errorMessage)`. Reused by `SavedConnectionsViewModel`.
- **Core / Keychain.**
  - `SavedConnectionsStore.swift` — per-entry Keychain item (`service = "com.applovin.yi.bedterm.savedConnections"`, `account = id.uuidString`), plus a `UserDefaults`-backed ordered index (`"savedConnections.order"` → `[UUID]`).
  - `CredentialsStore.swift` — kept temporarily only to read the legacy single-credential entry during migration; new writes go to `SavedConnectionsStore`.
- **Onboarding.** No behaviour change other than the `onFinish` destination.

## Key types

| Type | Role |
|---|---|
| `SavedConnection` | `Codable, Equatable, Identifiable`. `id: UUID`, `label: String` (may be empty — UI defaults to `user@host`), `credential: HostCredential`. |
| `SavedConnectionsStore` | `list() -> [SavedConnection]`, `load(id:) -> SavedConnection`, `save(_ entry: SavedConnection) throws`, `delete(id:) throws`, `migrateLegacyIfNeeded()`. Holds the ordered index in `UserDefaults`. |
| `ConnectAttempt` | `@MainActor`. `run(credential:) async -> Outcome`. `Outcome = .session(TerminalSession) / .mismatch(PendingMismatch) / .error(String, permissionDenied: Bool)`. Single owner of prewarm + `TerminalSession.connect` so both screens share one code path. |
| `SavedConnectionsViewModel` | `@Observable`. Loads + reorders the list, owns per-row state (`inFlightID`, `errorByID`), and exposes `add`, `edit`, `delete`, `connect(id:)`. |
| `ConnectionFormViewModel` | `@Observable`. Owns the form fields + validation + `save() throws -> SavedConnection.ID`. No connect logic. |

## Storage layout

Keychain items:

| Service | Account | Value |
|---|---|---|
| `com.applovin.yi.bedterm.savedConnections` | `<entry-uuid>` | JSON-encoded `SavedConnection` |
| `com.applovin.yi.bedterm.credentials` (legacy) | `default` | Existing single `HostCredential`. Read at migration, then deleted. |

`UserDefaults` keys:

| Key | Value |
|---|---|
| `savedConnections.order` | `[String]` of UUID strings, in display order. |

Reading the list is `order.compactMap { try? store.load(id: $0) }`; missing UUIDs are filtered (defensive against partial corruption).

## Connect path

`SavedConnectionsViewModel.connect(id:)`:

1. Sets `inFlightID = id`, clears the row's prior error.
2. `let entry = try store.load(id: id)`.
3. `let outcome = await connectAttempt.run(credential: entry.credential)`.
4. On `.session(session)` → publishes `lastSession`, the screen pushes `.terminal`.
5. On `.mismatch(pending)` → screen pushes `.hostKeyMismatch(...)`; trusting the new key calls `connect(id:)` again.
6. On `.error(msg, permissionDenied: pd)` → writes into `errorByID[id]`; the row renders the same `Open Settings / Retry` block today's screen does (re-usable view).
7. `defer { inFlightID = nil }`.

The `errorByID` map is keyed by entry id so only the affected row shows the error; tapping a different row clears the prior row's banner.

## Migration

`SavedConnectionsStore.migrateLegacyIfNeeded()` runs once on `SavedConnectionsScreen.task`:

1. If `UserDefaults.savedConnections.order` is non-empty, no-op.
2. Else attempt `CredentialsStore().load()`. On success, create a `SavedConnection(id: UUID(), label: "Last connection", credential: legacy)`, persist it, append to the index, and call `CredentialsStore().delete()`.
3. Errors are swallowed (logged via `os.Logger`), since a failed migration must not block the user from seeing an empty list and adding a new entry.

## Localization

New user-facing strings (all auto-extracted from SwiftUI literals or wrapped in `String(localized:)`):

- `"Saved Connections"` — nav title.
- `"Add Connection"` — empty-state primary action + `+` accessibility label.
- `"No saved connections yet"` — empty-state body.
- `"New Connection"` / `"Edit Connection"` — form nav titles.
- `"Label (optional)"` — form label field placeholder.
- `"Save"`, `"Cancel"`, `"Delete"`, `"Edit"` — toolbar / context actions.
- `"Delete \"\(label)\"? This will remove the saved password or key."` — delete confirmation (interpolated value only).
- `"Last connection"` — migration label.
- `"Couldn't load saved connections — device locked. Retry."` — locked-keychain banner.

Per `CLAUDE.md`, after edits to existing English strings (none here — this is all new copy), mark non-English entries `needs_review`.

## Design tokens

Per `CLAUDE.md` (Colors & appearance), every user-visible colour on the new surfaces comes from a named token in `Assets.xcassets` with both Light and Dark variants — no literal `Color(red:…)`, no system constants. Tokens this feature needs:

- `surface.list.background`, `surface.list.row` — list scroll background and row fill (reuse if already defined for the existing Connection form).
- `text.primary`, `text.muted` — row primary line and "user@host:port" subtitle.
- `badge.password.foreground` / `.background`, `badge.key.foreground` / `.background` — the auth-method badge in two flavours. Tokens, not raw palette names.
- `error.foreground` — inline row error (reuse the existing token if one exists; otherwise add).

If a needed token isn't in the catalogue yet, add it before wiring the view.

## Testing

Unit tests in `BedTermTests/SavedConnections/`:

- `SavedConnectionsStoreTests`
  - Save → list → load round-trip preserves all fields, including `privateKey` Data and passphrase.
  - Delete removes both index entry and Keychain item.
  - Migration creates exactly one entry from a populated legacy store, is idempotent on re-run, and is a no-op on a fresh install.
  - List order is preserved across reorders of the index array (set up via direct `UserDefaults` writes).
- `SavedConnectionsViewModelTests`
  - `add` / `edit` / `delete` reflect into `entries` and into a stub store.
  - `connect(id:)` flows through an injected `ConnectAttempt` stub: success → `lastSession` set; mismatch → `pendingMismatch` set; error → `errorByID[id]` populated; permission-denied flag propagates.
- `ConnectionFormViewModelTests`
  - Validation: missing host / port / username surface today's localized strings.
  - Save in `.add` mode appends a new entry; in `.edit` mode mutates the existing entry's fields without changing its `id`.
  - Empty label is preserved as empty (UI handles the default rendering).

Keychain is wrapped behind a small protocol seam in `Keychain.swift` so the store can be exercised against an in-memory dictionary in tests. The seam stays internal — the production `enum Keychain` keeps its current API.

## Out of scope (v1)

- Reordering / drag-to-rank / multi-delete.
- Folders, tags, colours.
- iCloud sync (kept off — secrets are `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`).
- Per-entry startup commands, port forwards, environment, jump hosts.
- Auto-sort by recency / usage counts.
- Import / export / share.
- Autofill from the system password manager.
