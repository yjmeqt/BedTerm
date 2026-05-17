# Rust + Metal Terminal Engine — Implementation Spec

## Overview

Replace SwiftTerm with:

| Layer | Technology | Replaces |
|-------|-----------|----------|
| Terminal emulation | Rust static lib wrapping `alacritty_terminal` | SwiftTerm `Terminal` parser |
| Rendering | Metal-backed `MTKView` (custom shaders) | SwiftTerm `TerminalView` |
| Input model | Warp-style block buffer with animation | PTY-passthrough send |

**Target:** iOS 26, 60 fps, ANSI colour-correct, scroll-ghost-free.

---

## 1. Project Layout

```
BedTermKit/
├── Package.swift                          # unchanged — remove SwiftTerm dep in phase 1
├── Sources/BedTermKit/
│   ├── Features/Terminal/
│   │   ├── TerminalScreen.swift           # minor: swap TerminalHostView → TerminalMetalView
│   │   ├── TerminalMetalView.swift        # NEW: MTKView UIViewRepresentable
│   │   ├── TerminalMetalRenderer.swift    # NEW: Metal render loop + glyph atlas
│   │   ├── TerminalFFI.swift              # NEW: Swift wrappers over C FFI types
│   │   ├── TerminalSession.swift          # MODIFIED: send() routes to InputBuffer
│   │   ├── InputViewModel.swift           # NEW: Warp block buffer state + commit
│   │   ├── ComposerView.swift             # NEW: Warp-style multiline editor
│   │   ├── KeyBar.swift                   # unchanged
│   │   └── KeyBarController.swift         # unchanged
│   └── CHeaders/
│       └── terminal_emu.h                 # GENERATED: cbindgen output
├── rust/
│   └── terminal_emu/
│       ├── Cargo.toml
│       ├── cbindgen.toml
│       ├── build.sh
│       └── src/
│           ├── lib.rs                     # FFI exports
│           ├── term.rs                    # Terminal struct (owns alacritty_terminal::Term)
│           ├── grid.rs                    # TerminalGrid repr(C) + dirty tracking
│           ├── input.rs                   # InputBuffer for Warp blocks
│           └── color.rs                   # ANSI palette ↔ RGB
└── Shaders/
    ├── terminal.metal                     # vertex + fragment for cells
    └── SharedTypes.h                      # shared structs between .metal and Swift
```

---

## 2. Rust Core (`terminal_emu`)

### 2.1 Crate Dependencies

```toml
[package]
name = "terminal_emu"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["staticlib"]

[dependencies]
alacritty_terminal = "0.24"
unicode-width = "0.2"
log = "0.4"

[profile.release]
lto = true
opt-level = "s"        # size-optimised for mobile
```

### 2.2 FFI Data Types

```rust
// grid.rs — repr(C) structs shared with Swift via terminal_emu.h

/// Per-cell attributes packed into 16 bits.
pub const FLAG_BOLD: u16       = 0b0000_0001;
pub const FLAG_ITALIC: u16     = 0b0000_0010;
pub const FLAG_UNDERLINE: u16  = 0b0000_0100;
pub const FLAG_INVERSE: u16    = 0b0000_1000;
pub const FLAG_DIM: u16        = 0b0001_0000;
pub const FLAG_STRIKETHROUGH: u16 = 0b0010_0000;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub ansi_index: u8,  // 0-255, or 0xFF for truecolor
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Cell {
    pub codepoint: u32,  // UTF-32, 0 = empty
    pub fg: Color,
    pub bg: Color,
    pub flags: u16,
    pub width: u8,       // 1 or 2 (CJK wide chars)
}

#[repr(C)]
pub struct DirtyRange {
    pub start_row: u32,
    pub end_row: u32,    // exclusive
}

#[repr(C)]
pub struct TerminalGrid {
    pub cells: *const Cell,         // flat array rows × cols
    pub cols: u16,
    pub rows: u16,
    pub total_scrollback: u32,
    pub viewport_top: u32,          // absolute row index of viewport row 0
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub cursor_visible: bool,
    pub cursor_style: u8,           // 0=block, 1=underline, 2=bar
    pub dirty: *const DirtyRange,
    pub dirty_count: u32,
}
```

### 2.3 FFI Function Table

```rust
// lib.rs

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Allocate a new terminal emulator with the given dimensions.
/// Returns null on OOM. Caller must free with term_destroy().
#[no_mangle]
pub extern "C" fn term_create(cols: u16, rows: u16, scrollback_limit: u32) -> *mut Term {
    let term = Term::new(cols as usize, rows as usize, scrollback_limit as usize);
    Box::into_raw(Box::new(term))
}

/// Free the terminal.
#[no_mangle]
pub extern "C" fn term_destroy(ptr: *mut Term) {
    if ptr.is_null() { return; }
    unsafe { drop(Box::from_raw(ptr)); }
}

/// Feed raw bytes into the parser. Returns the number of lines that became dirty.
/// The caller reads term_get_grid().dirty to find which rows to repaint.
#[no_mangle]
pub extern "C" fn term_feed(ptr: *mut Term, bytes: *const u8, len: u32) -> u32 {
    let term = unsafe { &mut *ptr };
    let slice = unsafe { std::slice::from_raw_parts(bytes, len as usize) };
    term.feed(slice)
}

/// Get a read-only snapshot of the grid. Valid until the next term_feed() or term_resize().
#[no_mangle]
pub extern "C" fn term_get_grid(ptr: *const Term) -> TerminalGrid {
    let term = unsafe { &*ptr };
    term.grid_snapshot()
}

/// Resize the terminal. Triggers a full-grid dirty.
#[no_mangle]
pub extern "C" fn term_resize(ptr: *mut Term, cols: u16, rows: u16) {
    let term = unsafe { &mut *ptr };
    term.resize(cols as usize, rows as usize);
}

/// Get selected text as a UTF-8 C string. Caller must free with term_string_free().
#[no_mangle]
pub extern "C" fn term_get_selection(ptr: *const Term) -> *mut c_char { ... }

/// Free a string returned by term_get_selection().
#[no_mangle]
pub extern "C" fn term_string_free(s: *mut c_char) {
    if s.is_null() { return; }
    unsafe { drop(CString::from_raw(s)); }
}

// ─── Input Buffer (Warp block) ───

#[no_mangle]
pub extern "C" fn input_buf_create() -> *mut InputBuffer { ... }

#[no_mangle]
pub extern "C" fn input_buf_destroy(ptr: *mut InputBuffer) { ... }

/// Append bytes to the buffer. Returns the new byte count.
#[no_mangle]
pub extern "C" fn input_buf_append(ptr: *mut InputBuffer, bytes: *const u8, len: u32) -> u32 { ... }

/// Get buffer contents. Caller must free with term_string_free().
#[no_mangle]
pub extern "C" fn input_buf_get_text(ptr: *const InputBuffer) -> *mut c_char { ... }

/// Clear the buffer.
#[no_mangle]
pub extern "C" fn input_buf_clear(ptr: *mut InputBuffer) { ... }
```

### 2.4 Internal Term Struct

```rust
// term.rs

use alacritty_terminal::term::Term as AlacTerm;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::ansi;

pub struct Term {
    inner: AlacTerm<EventProxy>,
    grid_cache: TerminalGrid,
    dirty_ranges: Vec<DirtyRange>,
}

struct EventProxy; // no-op listener — we poll the grid, not events

impl Term {
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Self { ... }
    pub fn feed(&mut self, bytes: &[u8]) -> u32 {
        // Track the range of rows that change during this feed
        let start = self.inner.grid().display_offset();
        self.inner.advance_parser(bytes);
        let end = self.inner.grid().display_offset() + self.inner.screen_lines();
        self.mark_dirty(start, end);
        self.dirty_ranges.len() as u32
    }
    pub fn grid_snapshot(&self) -> TerminalGrid { ... }
    pub fn resize(&mut self, cols: usize, rows: usize) { ... }
}
```

### 2.5 Build Script

```bash
#!/bin/bash
# rust/terminal_emu/build.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

TARGETS=(
    "aarch64-apple-ios"
    "aarch64-apple-ios-sim"
)

for target in "${TARGETS[@]}"; do
    echo "Building for $target..."
    cargo build --release --target "$target"
done

# Copy .a files and header to Xcode-accessible location
HEADER_DIR="$SCRIPT_DIR/../../Sources/BedTermKit/CHeaders"
mkdir -p "$HEADER_DIR"

# Copy headers generated by cbindgen
cbindgen --config cbindgen.toml --output "$HEADER_DIR/terminal_emu.h"

# Copy libs to a place Xcode can find via LIBRARY_SEARCH_PATHS
LIBS_DIR="$SCRIPT_DIR/../../libs"
mkdir -p "$LIBS_DIR"
cp target/aarch64-apple-ios/release/libterminal_emu.a "$LIBS_DIR/libterminal_emu-ios.a"
cp target/aarch64-apple-ios-sim/release/libterminal_emu.a "$LIBS_DIR/libterminal_emu-ios-sim.a"

echo "Done. Libraries in $LIBS_DIR, header in $HEADER_DIR"
```

---

## 3. Metal Renderer

### 3.1 TerminalMetalView (UIViewRepresentable)

```swift
// TerminalMetalView.swift

struct TerminalMetalView: UIViewRepresentable {
    let feed: AsyncStream<Data>
    let onSend: (Data) -> Void
    let onResize: (Int, Int) -> Void
    let animationDriver: AnimationDriver?  // nil when no animation active

    func makeUIView(context: Context) -> TerminalMetalUIView {
        let view = TerminalMetalUIView()
        view.renderer = TerminalMetalRenderer(device: view.device!, view: view)
        context.coordinator.start(consuming: feed, view: view)
        return view
    }
}

final class TerminalMetalUIView: MTKView {
    var terminal: OpaquePointer?          // Rust Term*
    var renderer: TerminalMetalRenderer!
    var inputBuffer: OpaquePointer?       // Rust InputBuffer*
    var animationProgress: Double = 0     // 0→1 for Warp block animation
    var activeBlockRange: Range<Int>?     // rows currently animating
}
```

### 3.2 Render Loop

```swift
// TerminalMetalRenderer.swift

final class TerminalMetalRenderer: NSObject, MTKViewDelegate {
    private let device: MTLDevice
    private let commandQueue: MTLCommandQueue
    private let atlas: GlyphAtlas
    private let pipelineState: MTLRenderPipelineState
    private var cellBuffer: MTLBuffer    // Cell[] GPU buffer
    private var lastGridHash: Int = 0     // for skipping identical frames

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
        // Recalculate cols × rows from drawable size / font size
    }

    func draw(in view: MTKView) {
        guard let termView = view as? TerminalMetalUIView,
              let term = termView.terminal,
              let drawable = view.currentDrawable else { return }

        let grid = term_get_grid(term)

        // Skip if nothing changed since last frame (saves battery)
        let hash = gridHash(grid)
        if hash == lastGridHash && termView.animationProgress == 0 { return }
        lastGridHash = hash

        // Upload dirty cells to GPU buffer
        uploadDirtyCells(grid, to: cellBuffer)

        // Ensure glyph atlas has every needed codepoint
        atlas.ensureGlyphs(for: grid)

        // Encode draw commands
        guard let cmd = commandQueue.makeCommandBuffer(),
              let encoder = cmd.makeRenderCommandEncoder(
                  descriptor: view.currentRenderPassDescriptor!
              ) else { return }

        encoder.setRenderPipelineState(pipelineState)
        encoder.setVertexBuffer(cellBuffer, offset: 0, index: 0)
        encoder.setFragmentTexture(atlas.texture, index: 0)
        // Per-frame uniforms: grid size, animation progress
        encoder.setVertexBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
        encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 0)

        let vertexCount = Int(grid.cols) * Int(grid.rows) * 6
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: vertexCount)
        encoder.endEncoding()

        cmd.present(drawable)
        cmd.commit()
    }
}
```

### 3.3 Glyph Atlas

```swift
// GlyphAtlas.swift

final class GlyphAtlas {
    private(set) var texture: MTLTexture      // 2048×2048 RGBA8
    private var occupancy: CGRectAllocator    // 2D bin-packing tracker
    private var glyphs: [UInt32: GlyphEntry] = [:]

    struct GlyphEntry {
        let atlasRect: CGRect       // normalised 0-1 coords for the shader
        let pixelSize: CGSize
    }

    func ensureGlyphs(for grid: TerminalGrid) {
        let cellCount = Int(grid.cols) * Int(grid.rows)
        let cells = UnsafeBufferPointer(start: grid.cells, count: cellCount)
        for cell in cells where cell.codepoint != 0 {
            if glyphs[cell.codepoint] == nil {
                rasterizeAndUpload(cell.codepoint)
            }
        }
    }

    private func rasterizeAndUpload(_ codepoint: UInt32) {
        // 1. CTFontCreateGlyphsForCharacters → CGGlyph
        // 2. CTFontCreatePathForGlyph → CGPath
        // 3. Render into CGBitmapContext (grayscale 8bpp)
        // 4. Upload bitmap row to MTLTexture.replace(region:...)
        // 5. Store atlasRect in glyphs dict
    }
}
```

### 3.4 Metal Shaders

```metal
// terminal.metal

#include <metal_stdlib>
using namespace metal;

// Must match grid.rs field order
struct Cell {
    uint codepoint;
    uchar4 fg;          // r, g, b, ansi_index
    uchar4 bg;
    ushort flags;
    uchar  width;
};

struct Uniforms {
    uint cols;
    uint rows;
    float2 cellSize;        // in pixels
    float2 atlasSize;       // 2048×2048
    float2 atlasInvSize;    // 1/2048
    float  animationProgress; // 0→1 for Warp block
    uint   animBlockStartRow;
    uint   animBlockEndRow;
    float  time;            // for cursor blink
};

struct VertexOut {
    float4 position [[position]];
    float2 texCoord;
    uchar4 fg;
    uchar4 bg;
    ushort flags;
};

vertex VertexOut cell_vertex(
    constant Uniforms &u [[buffer(1)]],
    uint vertexID [[vertex_id]]
) {
    // 6 vertices per cell (two triangles forming a quad)
    uint cellIndex = vertexID / 6;
    uint corner    = vertexID % 6;
    uint col = cellIndex % u.cols;
    uint row = cellIndex / u.cols;

    float2 basePos = float2(float(col) * u.cellSize.x,
                             float(row) * u.cellSize.y);
    // Warp animation: offset block rows upward during expansion
    float yOffset = 0.0;
    if (row >= u.animBlockStartRow && row < u.animBlockEndRow) {
        yOffset = (1.0 - u.animationProgress) * float(u.animBlockEndRow - u.animBlockStartRow) * u.cellSize.y;
    }

    // Quad vertex positions
    const float2 quadPos[6] = {
        {0, 0}, {1, 0}, {0, 1},  // tri 1
        {1, 0}, {0, 1}, {1, 1}   // tri 2
    };
    float2 offset = quadPos[corner] * u.cellSize;
    offset.y += yOffset;

    float4 clipPos;
    clipPos.xy = (basePos + offset) * 2.0 / float2(u.cols * u.cellSize.x, u.rows * u.cellSize.y) - 1.0;
    clipPos.y = -clipPos.y;  // flip Y for Metal NDC

    VertexOut out;
    out.position = clipPos;
    out.texCoord = quadPos[corner];  // placeholder, replaced per-glyph
    return out;
}

fragment float4 cell_fragment(
    VertexOut in [[stage_in]],
    texture2d<float> atlas [[texture(0)]],
    constant Uniforms &u [[buffer(0)]]
) {
    float4 glyph = atlas.sample(linearSampler, in.texCoord);

    float4 fg = float4(in.fg.rgb) / 255.0;
    float4 bg = float4(in.bg.rgb) / 255.0;

    // Inverse video swaps fg/bg
    if (in.flags & 0x0008) {
        float4 tmp = fg; fg = bg; bg = tmp;
    }

    // glyph.a drives the blend
    return mix(bg, fg, glyph.a);
}
```

### 3.5 Per-Cell Glyph UV Lookup

The vertex shader above uses placeholder UVs. In practice, each cell needs the UV quad of its glyph in the atlas. This is best done by:

**Option A (simpler):** Store per-cell UV quads in a separate GPU buffer, indexed by `vertexID`.
**Option B (faster):** Use an instanced draw where each instance = one cell, and the UV quad + position are computed from instance uniforms.

For Phase 1, Option A is recommended:

```swift
// Two buffers:
//   vertexBuffer[0] = cell data (codepoint, fg, bg, flags)
//   vertexBuffer[1] = per-cell UV quads (calculated CPU-side from atlas)
```

The UV quads are recalculated on the CPU whenever a new glyph enters the viewport. In steady-state (same characters displayed), the UV buffer is reused across frames.

---

## 4. Warp-Style Input Flow

### 4.1 State Machine

```
┌──────────┐   user taps Compose    ┌──────────────┐
│ PASSTHRU │ ─────────────────────→ │ COMPOSING    │
│ (default)│ ←───────────────────── │              │
└──────────┘   dismiss / cancel     │ buffer filled │
     │                              │ locally       │
     │                              └──────┬───────┘
     │ keystrokes go                   Enter │ or Send
     │ straight to PTY                       │
     │                                  ┌────▼──────────┐
     │                                  │ COMMITTING    │
     │                                  │ animation     │
     │                                  │ running       │
     │                                  └──────┬────────┘
     │                                  anim complete
     │                                  ┌────▼──────────┐
     │                                  │ PASSTHRU      │
     │                                  │ (remote       │
     │                                  │  output       │
     │                                  │  arriving)    │
     │                                  └──────────────┘
```

### 4.2 InputViewModel (Swift)

```swift
@MainActor
@Observable
final class InputViewModel {
    enum Mode {
        case passthrough
        case composing
        case committing(blockRange: Range<Int>)  // rows being animated
    }

    private(set) var mode: Mode = .passthrough
    private(set) var bufferText: String = ""
    private var rawBuffer: OpaquePointer         // Rust InputBuffer*

    let onCommit: (Data) -> Void                 // → TerminalSession.send()

    func handleKey(_ char: Character) {
        switch mode {
        case .passthrough:
            break // handled by TerminalSession.send() directly
        case .composing:
            let bytes = String(char).data(using: .utf8)!
            bytes.withUnsafeBytes { ptr in
                input_buf_append(rawBuffer, ptr.baseAddress!, UInt32(ptr.count))
            }
            bufferText = String(cString: input_buf_get_text(rawBuffer))
        case .committing:
            break // block input during animation
        }
    }

    func commit() {
        guard case .composing = mode else { return }
        let text = bufferText
        let data = text.data(using: .utf8)!

        // 1. Send to PTY as bracketed paste
        let bracketed = "\u{1B}[200~\(text)\u{1B}[201~"
        onCommit(bracketed.data(using: .utf8)!)

        // 2. Local echo: feed into terminal emulator
        term_extract_block_and_mark(
            terminal,
            rawBuffer,
            /* output: */ &animStartRow, &animEndRow
        )

        // 3. Start animation
        let blockRange = Int(animStartRow)..<Int(animEndRow)
        mode = .committing(blockRange: blockRange)
        input_buf_clear(rawBuffer)
        bufferText = ""

        // 4. Animation driver ticks progress 0→1 over ~300ms
        startAnimation(blockRange: blockRange)
    }

    func cancel() {
        input_buf_clear(rawBuffer)
        bufferText = ""
        mode = .passthrough
    }
}
```

### 4.3 Animation Driver

```swift
final class AnimationDriver {
    var progress: Double = 0       // 0 → 1
    var blockRange: Range<Int>?    // rows in the terminal grid

    private var displayLink: CADisplayLink?
    private let duration: TimeInterval = 0.3
    private var startTime: CFTimeInterval = 0
    private var onFrame: (AnimationDriver) -> Void

    func start(blockRange: Range<Int>, onFrame: @escaping (AnimationDriver) -> Void) {
        self.blockRange = blockRange
        self.progress = 0
        self.startTime = CACurrentMediaTime()
        self.onFrame = onFrame
        displayLink = CADisplayLink(target: self, selector: #selector(tick))
        displayLink?.add(to: .main, forMode: .common)
    }

    @objc private func tick() {
        let elapsed = CACurrentMediaTime() - startTime
        let t = min(elapsed / duration, 1.0)
        // ease-in-out cubic
        progress = t < 0.5 ? 4 * t * t * t : 1 - pow(-2 * t + 2, 3) / 2
        onFrame(self)
        if t >= 1.0 {
            displayLink?.invalidate()
            displayLink = nil
            onFrame = { _ in }
        }
    }
}
```

### 4.4 Block Accent Rendering

Committed blocks get a visual accent in the terminal. This is done in the fragment shader:

```metal
// Additional fragment shader logic (pseudocode):
// If current cell's row is the first row of a committed block:
//   Draw a 2px left border in the accent colour (blue/green/red based on exit code)
// If current cell's row is at the top of a block:
//   Optionally draw a faint timestamp text above
```

For Phase 1, the accent can be deferred. For Phase 3, the block metadata (start row, end row, accent colour) is passed to the shader via a structured uniform buffer.

---

## 5. SSH Data Flow Changes

### 5.1 New TerminalSession

```swift
@MainActor
@Observable
final class TerminalSession {
    // ... existing state, client, pumpTask unchanged ...

    let inputViewModel = InputViewModel()

    func userTyped(_ char: Character) {
        guard case .open = state else { return }
        if case .composing = inputViewModel.mode {
            inputViewModel.handleKey(char)
        } else {
            send(String(char).data(using: .utf8)!)
        }
    }

    func userPressedEnter() {
        if case .composing = inputViewModel.mode {
            inputViewModel.commit()
        } else {
            send(Data([0x0D]))  // \r
        }
    }

    func handleCommit(data: Data) {
        // Called by InputViewModel.commit() for the bracketed-paste payload
        send(data)
    }
}
```

### 5.2 Echo Suppression

Raw mode PTY by default disables remote echo. Citadel's PTY request already specifies terminal type `xterm-256color`, but the PTY mode (raw vs cooked) depends on the remote shell. Recommendation:

- After SSH connect, send `stty -echo raw` to force raw mode
- If the remote shell re-enables echo (e.g. during `read` builtin), the local emulator will naturally render the echoed characters — they just appear twice if the InputBuffer also does local echo

Phase 1 avoids this by not doing local echo at all. Phase 2 introduces local echo and must handle the race: one approach is to track the byte offset of committed text and suppress the first N bytes of remote output that match.

---

## 6. Xcode Build Integration

### 6.1 Adding a Run Script Build Phase

In the BedTermKit target:

```bash
# Run Script Phase: "Build Rust"
bash "${SRCROOT}/rust/terminal_emu/build.sh"
```

### 6.2 Linker Flags

```
OTHER_LINKER_FLAGS = $(inherited)
    -L"${SRCROOT}/libs"
    -lterminal_emu-ios          # or -lterminal_emu-ios-sim for simulator
```

### 6.3 Header Search Paths

```
HEADER_SEARCH_PATHS = $(inherited)
    "${SRCROOT}/Sources/BedTermKit/CHeaders"
```

### 6.4 Swift → C Bridging

No bridging header needed for a SwiftPM package target. Instead, use a module map:

```
// Sources/BedTermKit/CHeaders/module.modulemap
module TerminalEmu {
    header "terminal_emu.h"
    export *
}
```

Then in Swift:

```swift
import TerminalEmu

let term = term_create(80, 24, 10_000)
defer { term_destroy(term) }
```

---

## 7. Migration Phases

### Phase 1: Rust Core + Basic Metal (4–6 weeks)

**Goal:** Feature parity with SwiftTerm, but colours work and scroll is clean.

**Deliverables:**
- [ ] Rust `terminal_emu` builds for iOS, passes VT100 conformance tests
- [ ] Metal renderer draws text, ANSI 16/256/truecolor correct
- [ ] Cursor + blink work
- [ ] Scrollback + gesture scrolling clean (no ghosting)
- [ ] Dynamic Type font resizing
- [ ] Copy/paste (via UIPasteboard)
- [ ] All R1–R12 requirements still green
- [ ] SwiftTerm dependency removed from Package.swift

**Not yet implemented:**
- Warp block input (still PTY passthrough)
- R13 Composer (uses system keyboard dictation only)

### Phase 2: Warp Input + Composer (2–3 weeks)

**Goal:** Block-based input with composer UI.

**Deliverables:**
- [ ] Rust InputBuffer + FFI
- [ ] ComposerView (monospace, no autocorrect, multi-line)
- [ ] Block commit flow (bracketed paste → SSH, local echo into grid)
- [ ] Ctrl+C/D/Z/Esc passthrough from composer
- [ ] R13 rules all green
- [ ] R14 voice dictation inserts into InputBuffer

### Phase 3: Animation + Polish (2–3 weeks)

**Goal:** Warp-quality visual experience.

**Deliverables:**
- [ ] Block expansion animation (ease-in-out, 300ms)
- [ ] Block left-border accent (blue/green/red)
- [ ] Block timestamp display
- [ ] Block collapse for tall output
- [ ] Light/Dark palette live switching
- [ ] Performance: < 8ms GPU frame time at 120Hz
- [ ] Memory: glyph atlas < 20 MB, terminal grid < 50 MB for 10k scrollback

---

## 8. Testing Strategy

### Unit Tests (Swift Testing)

- `TerminalFFITests`: Verify `term_create` / `term_feed` / `term_get_grid` round-trip
- `GridSnapshotTests`: SGR colour sequences produce correct Cell.fg/bg
- `InputBufferTests`: Append, clear, get text
- `AnimationDriverTests`: Progress curve correctness, completion callback

### Snapshot Tests

- Feed known ANSI streams into the Rust core, capture `TerminalGrid`, assert cell colours match expected values
- Compare Metal-rendered frames against reference PNGs for key sequences (ls --color, vim, htop)

### Integration Tests

- UI test: connect to test host, run `ls --color=always`, verify coloured pixels in screenshot
- UI test: scroll terminal, verify no doubled glyphs in screenshot
- UI test: compose block, commit, verify block appears in terminal and remote shell executes command

---

## 9. Open Risks

1. **`alacritty_terminal` iOS compatibility** — Pre-flight: attempt `cargo build --target aarch64-apple-ios` before writing any wrapper code. If it fails, fall back to `vt100` crate (simpler, lighter, no POSIX deps).

2. **Glyph atlas sizing** — 2048×2048 holds ~4,000 32×32 glyphs. CJK locales may need >10,000 unique codepoints. Monitor atlas occupancy and implement LRU eviction if needed.

3. **MTKView vs SwiftUI composition** — `UIViewRepresentable` wrapping `MTKView` may have frame-pacing issues with SwiftUI's layout. Test with the full terminal screen + toolbar + keyboard animation before committing.

4. **Remote echo race** — Covered in Section 5.2. Worst case: command text appears twice for the first few commands after Phase 2. Fix before Phase 3.

5. **App Store size** — Rust static lib adds ~2–4 MB to the binary. Acceptable for the feature set. Monitor with `size` after each phase.
