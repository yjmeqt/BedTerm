# Swift ↔ Rust SSH Bridge

Design for the FFI surface that lets the (future) Rust-side `TerminalSession`
port drive Swift's `CitadelSSHClient` without dragging Citadel into Rust.

## Status

**Design + skeleton only.** No implementation has landed in this branch.

- Rust side (`rust-core/bedterm-ios-ui/src/ssh_bridge.rs`) defines the C
  ABI types (`BtSSHResultCode`, `BtSSHConnectRequest`, `BtSSHCompletion`,
  `BtSSHOutputSink`, `BtSSHClientVTable`), the `SSHBridgeHandle` owner,
  and the Rust-facing `SSHBridge` trait — all behind `#![allow(dead_code)]`.
  No C-exported register / release entry points exist yet; the handle is
  not constructible from Swift in this revision.
- Swift side (`BedTermKit/Sources/BedTermKit/Core/SSH/SSHClientBridge.swift`)
  is a skeleton: every method body is `fatalError("not implemented")` with
  a `TODO(W6)` comment pointing back at this design.
- The C header (`bedterm_ios_ui.h`) exposes the vtable/struct/enum
  typedefs but no register / release / session entry points yet.

The Rust `BtIosTerminalSession` port (and the FFI surface to drive it)
is **not** landed. Production SSH sessions continue to flow through
`TerminalScreen` + the Swift `TerminalSession`.

## Decision

**Option A — C function-pointer vtable.**

Swift fills a `BtSSHClientVTable` struct with `@convention(c)` thunks plus an
opaque `client_ctx` (a +1-retained `Unmanaged<SSHClientBridge>` pointer) and
hands it to Rust once per session via `bt_ios_ssh_register(...)`. Rust stores
the struct verbatim and calls through it.

Rationale (3 sentences):
- `SSHClient`'s public API is `async throws` + `AsyncStream<Data>`. Neither
  crosses `@objc` cleanly, so an ObjC-class wrapper would still need
  callback-style shims — Option B buys object identity we don't need.
- A flat C vtable is the smallest, most testable contract: no `msg_send!`,
  no ObjC dispatch from Rust, and a hard line between "owns Citadel" (Swift)
  and "owns terminal core + view" (Rust).
- It matches the direction we already established for Swift→Rust
  (`bt_ios_create_vc`), keeping one ABI style across the boundary.

## Swift-side wrapper

- File: `BedTermKit/Sources/BedTermKit/Core/SSH/SSHClientBridge.swift` (new).
- Class: `final class SSHClientBridge` — wraps a `SSHClient` instance plus the
  lifetime of the inbound-data pump task. Retained as
  `Unmanaged<SSHClientBridge>.passRetained(...).toOpaque()` and surfaced to
  Rust as the `client_ctx` field.
- Exported entry points (all `@_cdecl`, all callable on any thread; the impls
  hop to the appropriate actor as needed):
  - `bt_ssh_bridge_make(client_ptr) -> *mut c_void` — wrap an existing
    `SSHClient` (we already construct `CitadelSSHClient` in Swift) and hand
    Rust the vtable+ctx pair. The Swift side keeps a strong ref via the
    `Unmanaged` retain.
  - `bt_ssh_bridge_release(ctx)` — release the retain.
- vtable thunks (`@convention(c)` free functions):
  - `connect(ctx, req: BtSSHConnectRequest, completion, completion_ctx)`
  - `write(ctx, bytes_ptr, bytes_len, completion, completion_ctx)`
  - `resize(ctx, cols, rows, completion, completion_ctx)`
  - `disconnect(ctx, completion, completion_ctx)` — non-throwing.
  - `set_output_sink(ctx, sink, sink_ctx)` — installs Rust's data callback;
    Swift kicks off a `Task` that drains `client.output` and calls
    `sink(sink_ctx, ptr, len)` per chunk on the main queue.
- Async strategy: every operation is bridged through a Swift `Task` that
  awaits the underlying `SSHClient` call, then bounces back to the main
  queue and invokes the completion function pointer. Errors are mapped via
  `SSHErrorMapping` (see below) into a packed `BtSSHResultCode` + an
  out-`NSString` retained pointer.
- `AsyncStream<Data>` mapping: a single long-lived `Task` per bridge drains
  `client.output` and dispatches each chunk to the registered sink. The sink
  is called on the main queue (matches `TerminalSession`'s `@MainActor`
  expectation today; the Rust port's pump runs on the main thread).

Skeleton lives in `SSHClientBridge.swift`; every method body is
`fatalError("not implemented")` for now.

## Rust-side bindings

- File: `rust-core/bedterm-ios-ui/src/ssh_bridge.rs` (new).
- Module wired in via `mod ssh_bridge;` in `lib.rs`.
- Public Rust types:
  - `#[repr(C)] pub struct BtSSHClientVTable { … }` — fields mirror the
    Swift thunks listed above.
  - `pub struct SSHBridgeHandle { vtable: BtSSHClientVTable, ctx: *mut c_void }`
    — owned by whatever Rust code will host the future `TerminalSession`
    port; `Drop` calls the release thunk.
  - `pub trait SSHBridge` — Rust-facing trait the future `TerminalSession`
    port codes against (`connect`, `write`, `resize`, `disconnect`,
    `set_output_sink`). Implemented by `SSHBridgeHandle`. This isolates
    callers from raw FFI.
- Ownership: Swift retains `SSHClientBridge` for the lifetime of the vtable
  ctx pointer. Rust calls `(vtable.release)(ctx)` on `Drop`, balancing the
  retain. Function-pointer fields are stored by value; the vtable lives
  inside `SSHBridgeHandle` so it shares the handle's lifetime.

## Async strategy (no tokio)

- Each async op takes `(completion: extern "C" fn(ctx, code, msg_ns), ctx)`.
- Rust stores a `Box::into_raw(Box::new(SomeContinuation))` cast to
  `*mut c_void`. The completion thunk reconstructs the box and resumes a
  Rust-side state machine. Because we are single-threaded on the main queue,
  we can keep continuations in `Rc<RefCell<…>>` slots rather than building
  a futures executor.
- For `connect`, the continuation also primes `set_output_sink` so the data
  pump starts only after the channel is open — matches `CitadelSSHClient`'s
  current behaviour where the pump lives inside the `withPTY` closure.
- No `tokio` runtime, no `async-std`. The Rust port keeps the
  callback-driven pump style we already use for the back-tap callback in
  `vc.rs`.

## Error mapping

- Wire format: `BtSSHResultCode` (`u32`) + optional `*const NSString` for
  detail messages.
- Codes mirror `SSHError`:

  | Code | Variant |
  |------|---------|
  | 0    | `ok` |
  | 1    | `dnsResolution` |
  | 2    | `tcpRefused` |
  | 3    | `timeout` |
  | 4    | `handshakeFailed(msg)` |
  | 5    | `authenticationFailed` |
  | 6    | `privateKeyParse` |
  | 7    | `privateKeyPassphraseRequired` |
  | 8    | `hostKeyMismatch(stored, remote)` |
  | 9    | `disconnected(msg)` |
  | 10   | `peerReset` |
  | 11   | `shellExited(code)` |
  | 99   | `other` (catch-all; carries `String(describing:)`) |

- Variants that carry one string (`handshakeFailed`, `disconnected`, `other`)
  pass it via the `*const NSString` slot.
- `hostKeyMismatch` packs two strings as `"stored\nremote"` to keep the
  vtable narrow; Rust splits on the first newline.
- `shellExited(code)` packs the exit code as a non-zero return in a second
  out-param (`*mut i32`) — wired through the completion as a second arg.
- Strings are passed as **retained** `*const NSString` pointers (CFRetained
  via `Unmanaged.passRetained(...).toOpaque()`). Rust copies them to a Rust
  `String` via the `objc2-foundation::NSString` API already pulled into the
  crate, then releases the retain.

## TerminalSession Rust port outline

When the port lands, the Rust side maps 1:1 to the current Swift methods:

| Swift                              | Rust                                                | Calls bridge? |
|------------------------------------|-----------------------------------------------------|---------------|
| `init(client:hostID:persistence:)` | `TerminalSession::new(bridge, host_id, persistence)`| takes ownership of `SSHBridgeHandle` |
| `connect(credential:initialPTY:…)` | `TerminalSession::connect(...)`                     | yes — `vtable.connect` + `set_output_sink` |
| `send(data)`                       | `TerminalSession::send(bytes)`                      | yes — `vtable.write` (fire-and-forget completion) |
| `resize(cols:rows:)`               | `TerminalSession::resize(cols, rows)`               | yes — `vtable.resize` |
| `disconnect()`                     | `TerminalSession::disconnect()`                     | yes — `vtable.disconnect` |
| `recordKill(reason:)`              | local Rust helper                                   | no (persistence stays Swift for now, called via a separate small vtable later) |
| `describe(_:)` / `killReason(...)` | local Rust pure fns                                 | no |

`feed: AsyncStream<Data>` becomes a Rust `Receiver<Vec<u8>>` (single-threaded
broadcast) that the renderer view borrows. `blockStore` and `terminalCore`
remain Rust-side already — the port collapses today's "Swift session +
Rust core" split into one Rust object.

`PersistenceHandle` interactions (`attach`, `recordKill`) are out of scope
for *this* bridge and will get a parallel vtable when their turn comes.

## Migration order

1. **Land scaffolding (this change).** Empty Swift wrapper + Rust stubs +
   C header entries; everything compiles. No call sites yet.
2. **Wire vtable + register fn.** Implement the C ABI on both sides:
   `bt_ssh_bridge_make` returns a populated vtable; Rust trait calls
   through it; round-trip unit test in `BedTermKitTests` constructs a
   `MockSSHClient`-backed bridge and exercises every entry.
3. **Port `TerminalSession::send` + `resize`.** Smallest surface, no async
   completion plumbing needed beyond fire-and-forget.
4. **Port `TerminalSession::connect`.** Brings in the connect completion +
   output-sink dance.
5. **Port `disconnect` + state machine.** Including `recordKill` reason
   mapping and `closed(reason:)` strings.
6. **Swap call sites.** `TerminalContainerViewController` / Rust VC starts
   holding `TerminalSession` (Rust) instead of `TerminalSession` (Swift);
   delete the Swift file once nothing references it.
7. **Persistence vtable.** Parallel design doc when we get there — same
   pattern, different methods.

## Open questions

- Thread model: today `TerminalSession` is `@MainActor`. The Rust port will
  also be main-thread-only. Confirm we're OK keeping `client.output` drain
  on the main queue (current Swift code does); large bursts could in theory
  starve UI but we have no evidence of that yet.
- `bootstrapPayload`: passed as UTF-8 bytes or `*const NSString`? Default
  proposal: `*const c_char` (UTF-8, nul-terminated, owned by Rust for the
  duration of the connect call). Cheaper than NSString and the payload is
  always ASCII heredoc text.
- `MockSSHClient` parity: do we keep `MockSSHClient.swift` and run it
  through the bridge in tests, or build a Rust-side mock that fills the
  vtable directly? Lean toward "both" — Swift mock for end-to-end tests,
  Rust mock for unit tests of the port.
