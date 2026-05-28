# Rust Mock TTY — Design

**Status:** Draft
**Date:** 2026-05-17
**Branch:** `feat-mock-tty`

## Goal

Give the iOS app a no-SSH debug path that exercises the full terminal +
keyboard stack, and give `bedterm-core` an end-to-end Rust-side regression
suite over `alacritty_terminal` integration. The mock lives in Rust so a
single implementation serves both: Swift wraps it as an `SSHClient`; cargo
tests pipe it directly into `Term` and assert grid state.

## Non-goals

- Running a real shell on iOS (sandbox forbids process spawn).
- Faithfully emulating every `termios` flag — only the subset that affects
  observable terminal/keyboard behaviour is in scope.
- Anything reachable in Release builds. The whole subsystem is feature-gated
  off in Release; Release binaries must contain zero mock-TTY bytes.

## Architecture

The Rust crate gains a `mock_tty` module, gated by a new Cargo feature
`mock-tty`. The xcframework build script enables the feature for the Debug
configuration and disables it for Release.

```
rust-core/bedterm-core/
  Cargo.toml            # [features] mock-tty = []
  src/mock_tty/
    mod.rs              # MockTty handle, output buffer, FFI-facing state
    termios.rs          # ICANON/ECHO/ISIG/ONLCR/ICRNL + control chars
    buffer.rs           # Vec<u8> aggregation, drain to callback
    program.rs          # trait Program
    programs/
      echo_shell.rs     # built-in commands
      vim_lite.rs       # modal editor
      replay.rs         # asciinema .cast playback
      raw_sink.rs       # pure echo, for keyboard byte inspection
  src/ffi.rs            # adds the six bt_mock_tty_* symbols
  tests/
    loopback.rs         # MockTty ⇄ alacritty_terminal::Term
    termios.rs
    vim_lite.rs
    replay.rs
  fixtures/
    vim-edit.cast
    codex-tui.cast
    claude-code.cast
```

Swift side:

```
BedTermKit/Sources/BedTermKit/Core/SSH/RustMockTTYClient.swift   // #if DEBUG
BedTermKit/Sources/BedTermKit/Features/Hosts/HostsScreen.swift   // #if DEBUG section
```

### Data flow

1. User picks "Debug TTY → echo_shell" on the Hosts screen.
2. `RustMockTTYClient.connect()` calls `bt_mock_tty_create(program: 0, …)`
   and registers a C callback. The callback re-enters Swift on the calling
   thread and yields bytes into the client's `AsyncStream<Data>`.
3. `TerminalSession` reads that stream and feeds it to the existing terminal
   pipeline (Rust VTE, Metal renderer). No changes needed there.
4. Keyboard input from `ComposerBar` / `KeyBar` goes back through
   `client.write()` → `bt_mock_tty_write` → termios line discipline → active
   `Program::on_input` → outputs queued and drained via the callback.
5. `resize()` and an optional `tick(now_ms)` (driven by a Swift timer for
   replay programs only) follow the same path.

## FFI

Six symbols total. cbindgen exports them through the existing C header.

```c
typedef struct BtMockTty BtMockTty;
typedef void (*BtMockTtyOutputCallback)(const uint8_t* bytes,
                                        size_t len,
                                        void* user_data);

BtMockTty* bt_mock_tty_create(uint32_t program, const char* opts_json);
void       bt_mock_tty_free(BtMockTty*);
void       bt_mock_tty_set_output_callback(BtMockTty*,
                                           BtMockTtyOutputCallback cb,
                                           void* user_data);
int32_t    bt_mock_tty_write(BtMockTty*, const uint8_t* bytes, size_t len);
void       bt_mock_tty_resize(BtMockTty*, uint16_t cols, uint16_t rows);
void       bt_mock_tty_tick(BtMockTty*, uint64_t now_ms);
```

- `program` enum: `0 = echo_shell`, `1 = vim_lite`, `2 = replay`,
  `3 = raw_sink`. Unknown values default to `raw_sink`.
- `opts_json`: nullable C string. Only `replay` reads it
  (`{"cast_path": "..."}` or `{"cast_bytes_base64": "..."}`).
- All entry points are synchronous and `Send`-free at the FFI surface; the
  Swift wrapper guarantees calls are serialised by hopping to the
  `RustMockTTYClient`'s actor.
- `bt_mock_tty_tick` is a no-op for `echo_shell`/`vim_lite`/`raw_sink`. For
  `replay`, Swift drives it from a 60 Hz `Timer` (replay program internally
  decides whether enough wall-time has elapsed to release the next chunk).
- Output is delivered exclusively through the registered callback. Calls
  may happen reentrantly from within `bt_mock_tty_write`/`_resize`/`_tick`.
  Swift must not call back into the handle from inside the callback; it
  yields bytes to `AsyncStream` and returns.

## termios subset

Implemented in `termios.rs`. Defaults match a typical Linux `tcgetattr`
result for an interactive shell.

| Flag       | Default | Effect                                                   |
|------------|---------|----------------------------------------------------------|
| `ICANON`   | on      | Line-buffer input; backspace / `VERASE` edit buffer; CR commits a line |
| `ECHO`     | on      | Echo printable input bytes back to output                |
| `ECHOE`    | on      | On backspace, emit `\b \b` to erase the previous glyph   |
| `ISIG`     | on      | `VINTR` (0x03) raises `Program::on_signal(SIGINT)`       |
| `ICRNL`    | on      | Input CR → LF before delivery                            |
| `ONLCR`    | on      | Output LF → CRLF                                         |
| `VINTR`    | 0x03    | Default ^C                                                |
| `VERASE`   | 0x7F    | Default backspace                                         |
| `VEOF`     | 0x04    | At start of line, signals EOF to the program             |

A `Program` may call `termios.set_raw()` to clear `ICANON | ECHO | ICRNL`
in one shot, and `termios.restore()` to revert. `vim_lite` does this on
entry/exit. `replay` runs in raw mode the whole time (it is the program's
own job to emit echo bytes if needed — typically the cast file already
contains them).

Flags **not** implemented: `IXON` (flow control), `IEXTEN`, `TOSTOP`,
`PARENB`, baud rate, special chars beyond the table above. Programs that
need them are out of scope.

## Programs

### `raw_sink`

No shell behaviour. Every input byte is forwarded to output as-is (after
termios, which in this mode is set raw). Used to inspect exactly what the
`KeyBar`, soft keyboard, and IME send for each key. Not interesting for
visual rendering tests.

### `echo_shell`

Cooked-mode prompt `bedterm-debug$ `. Commands:

- `keys` — enter a sub-mode that prints each received byte as
  `0x1B  <ESC>` style lines, one per input byte, until `q`. Verifies
  `KeyBar`, ESC chord, arrow keys, IME composition bytes.
- `colors` — print a 16-colour bar, an 8×8 256-colour block, and a
  24-bit gradient strip. Verifies SGR handling at each depth.
- `cursor` — emit a sequence of CUP / CHA / DECTCEM operations
  (move-and-mark) that should leave a known checkerboard.
- `altscreen` — enter `ESC[?1049h`, draw a banner, wait 1.5 s
  (`tick`-driven), exit `ESC[?1049l`; verifies alt-screen save/restore.
- `demo` — alias `demo vim` / `demo codex` / `demo claude` — switches the
  active program to `replay` loaded with the named bundled fixture, then
  switches back to `echo_shell` when the fixture finishes.
- `stress` — random cursor positioning + truecolor SGR at high cadence for
  a few seconds. Manual Metal-renderer stability check.
- `vim` — switches the active program to `vim_lite`.
- `clear`, `exit` — standard.

Unknown input lines print `bedterm-debug: <name>: not found` and the
prompt re-appears.

### `vim_lite`

Minimal modal editor. Lives entirely in alt-screen. Buffer is an
in-memory `Vec<String>` seeded with a fixed welcome text. Status line on
the last row.

- Enter: `ESC[?1049h ESC[?25h ESC[H ESC[2J`, draw buffer + status, set
  cursor shape `ESC[2 q`, switch termios to raw.
- Exit: `ESC[?1049l`, restore termios, cursor shape `ESC[0 q`.
- NORMAL: `h j k l 0 $ x dd gg G i a o :` — `i`/`a`/`o` enter INSERT,
  `:` enters COMMAND.
- INSERT: printable bytes append, `0x7F` deletes left, `0x1B` returns to
  NORMAL, cursor shape `ESC[6 q` in INSERT.
- COMMAND: `:q` exits to `echo_shell`, `:w` flashes "written" on status,
  `:wq` does both, `ESC` aborts.
- `resize(cols, rows)` triggers a full redraw.

Purpose: verify ESC key delivery, mode-driven cursor shape rendering,
alt-screen save/restore, `:` command line redraw, and `resize` handling.

### `replay`

Loads an asciinema v2 `.cast` file (JSONL: header line + N rows of
`[time, "o", "bytes"]`). On each `tick(now_ms)`, releases every event
whose `time` ≤ wall-clock-elapsed-since-start. Hand-rolled parser; no
`serde` dependency. Out-of-band rows (`"i"`, marker, …) are skipped.

Fixtures live in `rust-core/bedterm-core/fixtures/` (used by cargo tests
via `include_bytes!`) and are mirrored into the iOS app bundle at
`BedTerm/Resources/DebugFixtures/` for runtime loading. A short
`scripts/copy-debug-fixtures.sh` keeps the two in sync; the Xcode build
phase invokes it for Debug only.

Initial fixtures:

- `vim-edit.cast` — open a file, `:set syntax=on`, edit, save, quit.
- `codex-tui.cast` — short Codex session showing synchronized output and
  truecolor.
- `claude-code.cast` — short Claude Code session showing OSC 8 hyperlinks
  and semantic prompt markers.

## Swift integration

`RustMockTTYClient` is `#if DEBUG`, conforms to the existing `SSHClient`
protocol with no protocol changes:

```swift
#if DEBUG
final class RustMockTTYClient: SSHClient { … }
#endif
```

- `connect()` calls `bt_mock_tty_create` with the program selected at
  init, registers the callback, and immediately transitions to "open".
  `request.credential` is ignored.
- `write()` forwards to `bt_mock_tty_write`.
- `resize()` forwards to `bt_mock_tty_resize`.
- `disconnect()` calls `bt_mock_tty_free` after detaching the callback.
- For `replay`, the client owns a `Timer` (60 Hz) that calls
  `bt_mock_tty_tick(currentMilliseconds)`; the timer is invalidated on
  disconnect.

The callback bridges to `AsyncStream<Data>.Continuation.yield`. The Swift
wrapper serialises FFI calls onto its own actor so the Rust core sees no
overlapping calls and the callback's "no reentry" rule is easy to honour.

UI: `HostsScreen` gains a `#if DEBUG` section "Debug TTY" with four rows
(echo shell, vim-lite, replay …, raw sink). The "replay …" row pushes a
picker for the bundled fixtures. Tapping a row constructs a
`TerminalSession` with the corresponding `RustMockTTYClient` and pushes
`TerminalScreen` — bypassing the credential form entirely. These strings
are developer-only and intentionally not localised.

## Build & gating

- `bedterm-core/Cargo.toml`: `[features] mock-tty = []`. The `mock_tty`
  module and the six FFI symbols are behind `#[cfg(feature = "mock-tty")]`.
- `scripts/build-rust-xcframework.sh` detects `${CONFIGURATION}` and
  appends `--features mock-tty` for `Debug` (and any `Debug*` variant);
  Release builds omit it. Verified by `nm` in CI: Release `.a` slices
  must not export `bt_mock_tty_*`.
- Swift code that calls these symbols is itself under `#if DEBUG`, so
  Release builds neither reference nor link the symbols.
- `cbindgen` runs unconditionally; the generated header always declares
  the symbols, but they only exist in the Debug-flavoured xcframework.

## Tests

### Rust (the main payoff of this design)

- `tests/loopback.rs` — for each `echo_shell` command and for `vim_lite`
  on a small scripted input, pipe `MockTty` output into
  `alacritty_terminal::Term`, then read `Term`'s grid and assert specific
  cells, cursor position, alt-screen state, and SGR attributes. This is
  the first end-to-end coverage of `term.rs` integration with the VTE.
- `tests/termios.rs` — ICANON line editing, VINTR raises signal, VEOF at
  line start, raw mode toggle round-trip.
- `tests/vim_lite.rs` — entering/leaving alt-screen, mode-driven cursor
  shape escapes, `:q` exits.
- `tests/replay.rs` — load a tiny in-repo `.cast`, drive `tick` with a
  fake clock, assert the released byte sequence.

`alacritty_terminal` is already a runtime dependency of `bedterm-core`
(used by `term.rs`). The mock module itself does **not** depend on it —
the mock is the byte producer; the parser is unrelated. The loopback
tests `use alacritty_terminal::Term` directly through the existing
dependency.

### Swift

- `RustMockTTYClientTests` — uses `raw_sink` to confirm bytes round-trip
  in/out and that disconnect drops the callback before `bt_mock_tty_free`.

## Risks & open considerations

- **Callback reentrancy**: the rule "do not call back into the handle
  from within the output callback" is policy, not enforced. The Swift
  wrapper's actor serialisation makes accidental violation hard but not
  impossible. We add a `#[cfg(debug_assertions)]` reentry-guard inside
  `MockTty` that panics if violated, so the rule is testable.
- **Replay clock drift**: 60 Hz tick is coarse for sub-frame events but
  acceptable; cast file timestamps are second-precision in practice.
- **Fixture freshness**: bundled `.cast` files will go stale as Claude
  Code / Codex evolve. Acceptable — they exist to exercise terminal
  features, not to be representative of current product UI.
- **`alacritty_terminal` API stability**: pinned at `0.26`. Tests are the
  only consumer of its public types; minor breakage is contained.

## What is intentionally out of scope

- TUI Probe HUD (the earlier "detect alt-screen / kitty-kbd / OSC 8"
  overlay). Deferred — none of the mechanism is in this spec; if useful
  it can sit on top later as a pure Swift sniffer over `client.output`.
- Asciinema recording (only playback). The replay program reads `.cast`
  files; it does not produce them.
- Cargo workspace split. The mock lives inside `bedterm-core` to keep
  the xcframework single-target.
