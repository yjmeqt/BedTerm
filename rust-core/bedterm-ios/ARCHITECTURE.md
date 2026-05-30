# bedterm-ios — architecture & ownership

The iOS UI layer of BedTerm: UIKit view controllers, Metal views, keyboard
routing, and the 7-function C FFI surface that Swift calls.

All pure application logic (models, state machines, SSH client, design
tokens, i18n) lives in **`bedterm-app`**. This crate is a thin UIKit/Metal
wrapper — it is gated wholesale on iOS (`#![cfg(target_os = "ios")]` in
`lib.rs`) and does not compile on macOS host.

## Ownership graph

```text
                   ┌──────────────────────────────────────────────┐
                   │         Swift host (SceneDelegate)            │
                   │                                              │
                   │  bt_ios_start_root_coordinator(window)        │
                   │  bt_ios_create_onboarding_flow_vc(...)        │
                   │  bt_ios_create_vc(...)                        │
                   │  bt_ios_vc_metal_view(...)                    │
                   │  bt_ios_view_feed_bytes(...)                  │
                   │  bt_ios_settings_onboarding_completed()       │
                   │  bt_ios_set_locale(...)                       │
                   └──────────────────────────────────────────────┘
                                       │
                                       ▼
   ┌──────────────────────────────────────────────────────────────┐
   │              RootCoordinator  (root_coordinator.rs)          │
   │                                                              │
   │  Owns: UIWindow, UINavigationController, toaster overlay,    │
   │  session handle. Drives hosts list → connect → push          │
   │  terminal flow. All VC lifecycle is Rust-owned.              │
   └──────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
   ┌──────────────┐  ┌────────────────┐  ┌──────────────────────┐
   │ HostsListVC  │  │ ConnectFormVC  │  │ RsTerminalVC (vc.rs) │
   │ (hosts_vc)   │  │ (connect_form) │  │                      │
   │              │  │                │  │  Owns:                │
   │  + toaster   │  │  + settings_vc │  │  - BtIosMetalInputView│
   │  + mismatch  │  │                │  │  - UITextView composer│
   └──────────────┘  └────────────────┘  │  - keybar             │
                                         │  - coordinator        │
                                         │  - ModeState          │
                                         └──────────────────────┘
```

## Threading model

**Single-threaded. Main actor only.** Every type that holds ObjC objects is
`MainThreadOnly` via `define_class!`. `Rc<ModeState>` (not `Arc`) enforces
main-thread confinement.

Raw-pointer ivars (`Cell<*const AnyObject>`) are weak / observer references.
Children observe the VC through raw pointers, never through `Retained<T>`
(which would create a retain cycle).

## FFI surface

Exactly **7** `#[no_mangle]` exports, all called from Swift:

| Export | Defined in | Swift caller |
|--------|-----------|-------------|
| `bt_ios_set_locale` | `bedterm-app/src/l10n.rs` | `AppDelegate` |
| `bt_ios_settings_onboarding_completed` | `ffi/settings.rs` | `SceneDelegate` |
| `bt_ios_start_root_coordinator` | `root_coordinator.rs` | `SceneDelegate` |
| `bt_ios_create_onboarding_flow_vc` | `ffi/onboarding.rs` | `SceneDelegate` |
| `bt_ios_create_vc` | `ffi/vc.rs` | `SceneDelegate` |
| `bt_ios_vc_metal_view` | `ffi/view.rs` | `SceneDelegate` |
| `bt_ios_view_feed_bytes` | `ffi/view.rs` | `SceneDelegate` |

All other functions that used to be `#[no_mangle]` (VC release, hosts VM
operations, terminal session lifecycle, shell integration, network utilities,
bedterm-core terminal state) are now normal `pub(crate)` Rust functions —
they were only ever called from other Rust code.

The generated C header lives at `include/bedterm_ios.h` and is produced by
cbindgen (configured in `cbindgen.toml`, invoked by `build.rs`). It walks
`bedterm-app` for `bt_ios_set_locale`; bedterm-core is not walked (no FFI
exports there anymore). `item_types = ["functions", "typedefs"]` prevents
`pub const` leakage.

## Module inventory

All modules are iOS-only (crate-level `#![cfg(target_os = "ios")]`).

| Module | What it is |
|--------|-----------|
| `vc` | `RsTerminalViewController` — the terminal session VC |
| `metal_view` | `BtIosMetalInputView` — Metal-backed terminal canvas |
| `metal_cursor_layer` | Blinking cursor overlay (CAShapeLayer) |
| `metal_selection_layer` | Text selection overlay (re-exports SelectionRange from bedterm-app) |
| `coordinator` | `BtIosKeyboardCoordinator` — focus routing, key dispatch |
| `input_mode` | `ModeState` — 3-state input mode (re-exports InputMode from bedterm-app) |
| `keybar` | Bottom keybar strip (tab/esc/ctrl/dpad/send/composer) |
| `action_chip` | Keybar button factory |
| `composer_text_view` | `UITextView` subclass for composer mode |
| `connecting_overlay` | Spinner overlay during SSH connect |
| `disconnect_banner` | "Disconnected" banner |
| `debug_hud` | DEBUG-only HUD for input mode switching |
| `dpad` | On-screen d-pad for terminal navigation |
| `toaster` | `BtIosToasterView` — toast notifications (pure types in bedterm-app) |
| `prompt_context_chips` | CLI agent / context chips in the input bar |
| `block_list_composer` | Block-list mode composer integration |
| `block_list` | `BtIosBlockListViewController` (pure submodules in bedterm-app) |
| `ime_preedit_overlay` | IME preedit text overlay |
| `design_system` | UIColor factories, components, typography (pure tokens in bedterm-app) |
| `hosts` | `BtIosHostsListViewController` (model in bedterm-app) |
| `connect_form` | `BtIosConnectFormViewController` (model in bedterm-app) |
| `onboarding` | Onboarding flow VCs + coordinator (state in bedterm-app) |
| `root_coordinator` | `RootCoordinator` — app-level nav orchestration |
| `host_key_mismatch_vc` | Host-key mismatch review VC |
| `hosts_flow_controller` | Legacy flow controller |
| `hosts_store` | Keychain-backed persistence for saved hosts |
| `settings_store` | NSUserDefaults-backed settings |
| `settings_vc` | Settings sheet VC |
| `terminal_session` | SSH connect → open_shell → byte pump lifecycle |
| `terminal_palette` | Terminal colour palette |
| `text_input` | `UITextInput` protocol helpers |
| `tokens` | Asset-catalog token helpers |
| `a11y` | Accessibility identifier helpers |
| `ffi` | FFI namespace (onboarding, settings, vc, view submodules) |
| `l10n` | (re-exports from bedterm-app) |
| `geometry` | (re-exports from bedterm-app) |

## Anti-patterns

- **No new `#[no_mangle]` unless Swift calls it.** Internal Rust-to-Rust
  calls go through normal `pub(crate)` functions.
- **No retain cycles via `Retained<T>` back-pointers.** Children observe
  the VC through `Cell<*const AnyObject>`.
- **No `Arc` / `Mutex` / `RwLock`.** Main-thread-only invariant.
- **No parallel ownership of `BtTerm`.** Exactly one `BtIosMetalInputView`
  owns the terminal grid.

## See also

- `bedterm-app/src/lib.rs` — pure application logic
- `bedterm-core/src/lib.rs` — terminal engine
- `cbindgen.toml` — C header generation config
