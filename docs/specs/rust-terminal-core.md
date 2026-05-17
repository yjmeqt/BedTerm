# Rust Terminal Core — Spec

## Purpose
Replace SwiftTerm's VTE parser + grid + scrollback with a Rust core built on
`alacritty_terminal`. This document spec is the long-form companion to plan
`2026-05-17-rust-terminal-core.md` and the future plan B `metal-terminal-renderer`.

## Why Rust + alacritty_terminal
- Battle-tested parser; expected to resolve the `ansi_colors_lost_with_metal`
  bug class for the eventual Metal renderer.
- Clean separation of parser/grid from rendering — fits a phone UI better than
  SwiftTerm's UIKit-coupled architecture.
- Same parser engine as Alacritty, Wezterm, and Zed's terminal panel.

## Scope (this plan)
- Cargo workspace under `rust-core/` with `bedterm_core` crate.
- C ABI (`bt_term_new/feed/resize/snapshot/free`).
- xcframework for `aarch64-apple-ios`, `aarch64-apple-ios-sim`,
  `aarch64-apple-darwin`. Catalyst slice deferred.
- Swift `TerminalCore` + `GridSnapshot` types.
- Shadow harness comparing core vs SwiftTerm on a 5-fixture corpus.
- Debug-only live shadow inside `TerminalHostView`.

## Out of scope (deferred to Plan B / C)
- Metal renderer.
- Removing SwiftTerm dependency.
- OSC 133 / block model.
- Reflow on resize.
- Selection model.
- Bracketed paste handling at the core level (SwiftTerm still does this).

## Threading model
`TerminalCore` is single-threaded. All access happens from the main actor
(matching today's `TerminalHostView.Coordinator.start(consuming:)`).

## Memory model
- Snapshot cells live in Rust, lifetime-bound to the handle.
- `bt_term_snapshot` returns a borrowed pointer valid until the next
  feed/resize/snapshot/free OR an explicit `bt_term_snapshot_release`.
- Swift copies into a `[Cell]` array before returning — no dangling pointers
  cross the FFI boundary into application code.

## Implementation notes (carried from execution)

### alacritty_terminal version
The plan called for `alacritty_terminal = "0.24"`. That version transitively
depends on two incompatible `rustix` major versions on Rust 1.94 and fails to
compile. The crate was bumped to `"0.26"`, which consolidates on `rustix v1.x`.
API differences from 0.24 that mattered:
- `Processor::advance` takes `&[u8]`, not a single byte at a time.
- Cells store `c: char` rather than a raw `u32`.
- `Flags::ALL_UNDERLINES` is the bit-union covering all underline styles —
  matched as `intersects` so single, double, undercurl, dotted, and dashed
  all set the snapshot's underline bit.

### Swift module naming
SwiftPM's binaryTarget is named `BedTermCore`, but Swift code uses
`import BedTermCoreC`. The xcframework no longer carries a `module.modulemap`;
the module is instead provided by a sibling SwiftPM C target at
`BedTermKit/Sources/BedTermCoreC/`, with the cbindgen-generated header copied
into its `include/` directory. The build script keeps the header in sync.

### Opaque pointer typing
cbindgen emits `BtTerm` as an opaque forward-declared struct. The Swift Clang
importer maps this to `OpaquePointer`, not `UnsafeMutablePointer<BtTerm>`.
`TerminalCore.handle` is typed as `OpaquePointer`.

## Known parser divergences
Recorded as the shadow harness flags them. Each entry: fixture, location,
SwiftTerm output, Core output, decision (accept divergence / file bug).

### Resolved during Task 10
- `04_utf8_mixed.bin` at `(16, 0)`: SwiftTerm's `CharData.getCharacter()` could
  not resolve the emoji because the codepoint lives in SwiftTerm's private
  `indexToCharMap`. Test was updated to call the `Terminal.getCharacter(col:row:)`
  public API instead. Not a parser divergence — a SwiftTerm API gotcha.

(Empty otherwise at plan-completion time; populated during shadow runs in
real-world use.)
