# bedterm-ios — architecture & ownership

This crate is the **iOS UI layer** of BedTerm: a set of UIKit/Metal classes
(implemented via `objc2`'s `define_class!`) that the Swift host instantiates
through a small C FFI surface. The crate owns the terminal view controller,
its Metal-backed input view, the keyboard router, the block-list view
controller, and the persistent input-mode state. It does **not** own the
SSH session, the data layer, or the navigation stack — those live in Swift.

The single biggest source of subtlety here is **mixed ownership across the
Swift / Rust / ObjC boundary**: UIKit retains views by way of the view
hierarchy, Rust holds some objects as `Retained<T>` (strong) and others as
`Cell<*const _>` (weak / observer). The strong edges are mostly obvious;
the raw-pointer edges are not, so they are the focus of this document.

## Ownership graph

```text
                   ┌────────────────────────────────────────────────────┐
                   │              Swift host (UINavigationController)    │
                   │                                                      │
                   │   IosTerminalView  ─ retains ─→  RsTerminalVC*       │
                   └────────────────────────────────────────────────────┘
                                       │ strong (+1 via bt_ios_create_vc)
                                       ▼
   ┌──────────────────────────────────────────────────────────────────────┐
   │                  RsTerminalViewController  (the VC)                  │
   │  ───────────────────────────────────────────────────────────────     │
   │                                                                      │
   │   Ivars (RefCell<Option<Retained<…>>> = strong unless marked):       │
   │     view1                ──→ BtIosMetalInputView                     │
   │     view2                ──→ UITextView (composer)                   │
   │     keybar / keybar_nl   ──→ UIView (chip strip)                     │
   │     send/composer/close  ──→ UIButton (action chips)                 │
   │     view1_tap            ──→ UITapGestureRecognizer                  │
   │     coordinator          ──→ BtIosKeyboardCoordinator                │
   │     debug_hud            ──→ BtIosDebugHUD                           │
   │     mode_state           ──→ Rc<ModeState>                           │
   │     on_back / ctx        ──  C callback + opaque ctx (no ownership)  │
   │                                                                      │
   │   ────────────────────────────────────────────────────────────       │
   │   Outgoing raw pointers (weak, set after construction):              │
   │                                                                      │
   │   ┌── ModeState.vc ────────────────────────────────────┐             │
   │   │   *const AnyObject → self (the VC).                │             │
   │   │   Used to msg_send `modeStateDidChange` on every   │             │
   │   │   mode mutation. Set in `viewDidLoad`, never       │             │
   │   │   cleared (VC outlives Rc<ModeState>).             │             │
   │   └────────────────────────────────────────────────────┘             │
   └──────────────────────────────────────────────────────────────────────┘
              │ strong            │ strong              │ strong
              ▼                   ▼                     ▼
   ┌────────────────────┐  ┌─────────────────────┐  ┌──────────────────┐
   │ BtIosMetalInputView│  │ BtIosKeyboardCoordr │  │  Rc<ModeState>   │
   │  (= view1)         │  │                     │  │                  │
   │                    │  │  Ivars:             │  │  mode / kbd-vis  │
   │  Ivars:            │  │   view1 ┄→ AnyObj   │  │  vc ┄→ AnyObj    │
   │   input_delegate   │  │   view2 ┄→ AnyObj   │  │                  │
   │     ┄→ UIKit's     │  │   ctrl_button (◆)   │  │  Cloned `Rc`     │
   │     UITextInput-   │  │   mode_state (◆ Rc) │  │  handles live    │
   │     Delegate       │  │                     │  │  on VC, coord,   │
   │   coordinator      │  │  (◆ = strong)       │  │  metal view, HUD │
   │     ┄→ Coordinator │  │                     │  │                  │
   │   mode_state       │  └─────────────────────┘  └──────────────────┘
   │     ┄→ ModeState   │            ▲ weak                ▲
   │   term: *mut BtTerm│            │ (view1.coordinator) │ (Rc clones)
   │     (◆ owned;      │            └─────────────────────┘
   │      bt_term_free  │
   │      in Drop)      │
   └────────────────────┘

   Block-list path (separate VC, instantiated by Swift host directly):

   ┌────────────────────────────────────┐
   │  BtIosBlockListViewController       │
   │   Ivars:                            │
   │     session           ┄→ TerminalSession (Swift, legacy)
   │     shared_metal_view ┄→ BtIosMetalInputView (parent VC retains)
   │     content_view, recognizers (◆ Retained)
   │     block_source: Box<dyn BlockSource>  ◆
   └────────────────────────────────────┘

   SSH bridge handle (lifetime decoupled from the VC):

   ┌───────────────────────────────────────────────────────────────┐
   │  SSHBridgeHandle    (heap, owned by Swift via Box::into_raw)   │
   │    vtable: BtSSHClientVTable  (copied by value at register)    │
   │    ctx:    *mut c_void  ── opaque Swift handle; released via   │
   │                            vtable.release(ctx) in Drop         │
   └───────────────────────────────────────────────────────────────┘
```

Legend:
- `──→` strong ownership (Rust `Retained<T>` / `Box<T>` / `Rc<T>`, or UIKit
  retain via the view hierarchy).
- `┄→`  weak / raw-pointer reference. The pointee is owned elsewhere; the
  pointer is an observer.
- `(◆)` strong; called out where the field name is ambiguous.

### Why this shape

The VC is the **single root of truth** for view ownership. Every UIView it
mounts is retained twice — once by the VC's `RefCell<Option<Retained<…>>>`
ivars (so we can re-frame / re-tint without re-querying the hierarchy) and
once by UIKit through the subview tree. The redundant strong refs are
intentional: they decouple Rust ivars from UIKit's removal semantics so
`removeFromSuperview` doesn't drop the Rust handle.

Children (the coordinator, the metal view, the mode state) need to talk
*back* to the VC and to each other for input routing. We deliberately do
not give them `Retained<T>` of the parent, because that would create a
retain cycle — the VC owns them. Instead they hold `Cell<*const AnyObject>`
or `Cell<*const ModeState>` weak refs, set after construction.

## Lifecycle ordering

1. Swift host calls `bt_ios_create_vc` → produces a `+1` retained
   `RsTerminalViewController *`. **No subviews exist yet** — `viewDidLoad`
   hasn't fired.
2. The host installs the VC on a `UINavigationController` (or via SwiftUI's
   `UIViewControllerRepresentable`). UIKit calls `viewDidLoad`.
3. `viewDidLoad` constructs view1 (`BtIosMetalInputView`), view2
   (`UITextView`), the keybar variants, all action chips, the coordinator,
   the `Rc<ModeState>`, the debug HUD. **Inside `viewDidLoad`** it wires
   the weak back-pointers:
   - `coordinator.set_view1/view2` (raw pointers into the parent's
     subviews — UIKit retains them now).
   - `view1.set_coordinator(*const coordinator)`.
   - `view1.set_mode_state(Rc::as_ptr(&mode_state))`.
   - `ModeState::vc` → `self` (the VC, as `*const AnyObject`).
4. Only **after** `viewDidLoad` is it safe for the host to call
   `bt_ios_vc_metal_view` / `bt_ios_vc_coordinator`. The Swift wrapper
   `IosTerminalView::makeVC` calls `loadViewIfNeeded()` *before* exposing
   the VC pointer, which forces `viewDidLoad` to fire synchronously and
   makes the post-`viewDidLoad` precondition trivially true.
5. On teardown: Swift releases the VC (`bt_ios_release_vc` → drops the
   `+1`). UIKit removes the subviews; the VC's `Drop` (implicitly via
   `Retained::from_raw` + scope) drops every `RefCell<Option<Retained<…>>>`,
   which drops the children. The children's `Cell<*const _>` ivars dangle
   for exactly one operation — the `dealloc` chain — and are never read
   afterwards because UIKit has already removed them from the responder
   chain and the recognizer targets.

The "VC outlives children" invariant is enforced by **construction order**:
parents are built first, children second, weak pointers always go
parent → child. The only weak pointer in the other direction is
`ModeState::vc`, which is safe because `Rc<ModeState>` is uniquely owned
by the VC (the other holders are clones, and clones can't outlive the
original — the original lives in `Ivars::mode_state`, which is dropped
last in the VC's drop order).

## Threading model

**Single-threaded. Main actor only.** Every type in this crate that holds
ObjC objects is `MainThreadOnly` via `define_class!`. The pure-logic
modules (`scroll_physics`, `input_mode`, `block_list/layout`, …) don't
spawn threads of their own.

This is why:

- **`Rc<ModeState>`, not `Arc<ModeState>`.** Cross-thread sharing is
  forbidden, so the atomic refcount of `Arc` is pure overhead. `Rc`
  panics if cloned across threads, which is the diagnostic we want.
- **`Cell<T>` is fine for ivars.** `Cell` gives `!Sync` for free, which
  matches the main-actor invariant. Mutation through `&self` is safe
  because no other thread can be racing the read.
- **`unsafe impl Send for Ivars`** appears on the `MainThreadOnly`
  classes. This is a **lie required by objc2**: `define_class!` needs
  `Ivars: Send + Sync` for the class registration path, but the
  `#[thread_kind = MainThreadOnly]` annotation guarantees ObjC will only
  vend these objects from the main thread. The unsafe impls are sound
  because the runtime never actually ships these ivars to another
  thread.
- **Raw pointers in `Cell<*const _>` are main-thread-only by the same
  argument.** No atomics, no fences, no SeqCst — the producer of the
  pointer (the VC's `viewDidLoad`) and every consumer run on the main
  thread.

## FFI surface

All `bt_ios_*` C exports are gated on `target_os = "ios"`. They live in
two namespaces:

### `crate::ffi::vc` — view-controller lifecycle (called once per session)

| Export                  | Description                                              | Swift caller                  |
| :---------------------- | :------------------------------------------------------- | :---------------------------- |
| `bt_ios_create_vc`      | Allocate + init the VC; returns `+1` retained `void *`.  | `IosTerminalView::makeVC`     |
| `bt_ios_release_vc`     | Drop the `+1`. Idempotent for null.                      | `IosTerminalView::dismantle`  |

### `crate::ffi::view` — metal-view byte/event plumbing (called per frame / per key)

| Export                       | Description                                                              | Swift caller                                |
| :--------------------------- | :----------------------------------------------------------------------- | :------------------------------------------ |
| `bt_ios_vc_metal_view`       | Resolve the embedded `BtIosMetalInputView *` (or NULL in headless test). | `IosTerminalView` after `loadViewIfNeeded`. |
| `bt_ios_view_feed_bytes`     | Push raw PTY bytes into the view's owned `BtTerm`.                       | `TerminalSession.onOutput`                  |
| `bt_ios_view_set_on_send`    | Install C-callback PTY sink (HW keys, IME commits go out here).          | `TerminalSession` wiring at attach time     |
| `bt_ios_view_set_on_resize`  | Install C-callback fired on cols/rows changes.                           | `TerminalSession` wiring at attach time     |
| `bt_ios_view_grid_dim`       | Read the most recent (cols, rows) the view derived.                      | Session resize handshake                    |
| `bt_ios_view_cell_size_px`   | Read the renderer atlas cell pixel size.                                 | Scroll-surface sizing in Swift              |

### `crate::ssh_bridge` — Swift-built SSH client registered into Rust

| Export                          | Description                                                       | Swift caller                |
| :------------------------------ | :---------------------------------------------------------------- | :-------------------------- |
| `bt_ios_register_ssh_bridge`    | Copy a `BtSSHClientVTable` + take ownership of an opaque `ctx`.   | `SSHClientBridge.register`  |
| `bt_ios_ssh_bridge_release`     | Drop the handle; vtable's `release(ctx)` fires from Rust's Drop.  | `SSHClientBridge.shutdown`  |
| `bt_ssh_release_message`        | Swift-side import; balances `+1` retained `NSString *` details.   | (Rust → Swift extern call.) |

No other `#[no_mangle]` items exist in the crate (verified by grep).
`bt_ssh_release_message` is an **import**, not an export: it appears in
the table because it's part of the same SSH-bridge contract.

## Pure-logic vs UIKit-coupled modules

`lib.rs` partitions every module into one of two tiers. The tier
determines whether the module compiles on a macOS host (for
`cargo test -p bedterm-ios`) or only on iOS targets.

| Tier        | Modules                                                                                                                                                                                                                                                                                                                                                       |
| :---------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Pure logic  | `block_header`, `block_panel_style`, `color`, `display_mode`, `geometry`, `input_mode`, `metal_selection_layer` (the `SelectionRange` value type), `prompt_context`, `scroll_physics`, and most of `block_list/*` (`layout`, `scroll`, `selection`, `sticky`, `source` value types).                                                                          |
| UIKit/Metal | `a11y`, `action_chip`, `block_list_composer`, `composer_text_view`, `connecting_overlay`, `coordinator/*`, `debug_hud`, `disconnect_banner`, `dpad`, `ime_preedit_overlay`, `keybar`, `metal_cursor_layer`, `metal_view/*`, `prompt_context_chips`, `ssh_bridge`, `terminal_palette`, `text_input`, `tokens`, `vc`, `block_list/vc`, `block_list/mod` (class). |

The pure tier compiles unconditionally; the UIKit tier is
`#[cfg(target_os = "ios")]`. The split is enforced by `lib.rs` rather
than a feature flag so the test command stays a one-liner.

## Anti-patterns

Things to **not** do here. Each of these caused a real bug or a real
refactor at some point.

- **No new god-class.** The original `metal_view.rs` accumulated ~3 kloc
  of UITextInput + IME + selection + scroll + key encoding before it was
  split across `metal_view/{text_input,ime,gestures,keys,render,…}`.
  Selectors in `metal_view/mod.rs` are one-line forwarders to per-concern
  `do_*` helpers; keep it that way. The same shape applies to
  `coordinator/`.
- **No parallel ownership of `BtTerm`.** Exactly one `BtIosMetalInputView`
  owns the terminal grid (and only when `owns_term == true`). The
  `set_external_term` path exists precisely to avoid double-free when the
  Swift host wants to share a grid; do not introduce a second `*mut BtTerm`
  in any other ivar.
- **No retain cycles via `Retained<…>` back-pointers.** Children observe
  the VC / each other through `Cell<*const AnyObject>`. Never replace a
  weak ivar with `Retained<T>` "to make lifetimes nicer" — it will keep
  the VC alive past dismissal and break navigation pops silently.
- **Don't read raw-pointer ivars before they're set.** Every `Cell<*const _>`
  starts at null. Every consumer **must** null-check
  (`if !ptr.is_null()` or `Some(ptr).filter(|p| !p.is_null())`) before
  dereferencing. This is documented per-ivar in the field-level
  SAFETY comments — read those before adding a new consumer.
- **No `#[no_mangle]` outside `crate::ffi` or `crate::ssh_bridge`.**
  The header in `include/bedterm-ios.h` enumerates the entire C
  surface; new exports go through `ffi::vc` or `ffi::view` and get a
  matching declaration in the header.
- **No `Arc` / `Mutex` / `RwLock`.** If you reach for one, you've broken
  the main-thread-only invariant; back up and find the actual race.

## See also

- `metal_view/mod.rs` — `Ivars` doc-comments enumerate every raw-pointer
  field with its SAFETY contract.
- `coordinator/mod.rs` — same for the keyboard router.
- `input_mode.rs` — `ModeState::vc` documents the only weak pointer that
  points *from* a child *to* the VC.
- `vc.rs` — strong-ref ivars; the `ctx` callback pointer is documented
  there.
- `block_list/vc.rs` — block-list VC's two weak refs (`session`,
  `shared_metal_view`).
