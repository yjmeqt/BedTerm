# BedTerm Render CLI + Testing Methodology

Design for an offline macOS CLI that renders terminal frames to PNG, enabling visual
regression testing and fast developer iteration without booting an iOS simulator.

## Architecture

A new `[[bin]]` target in `rust-core/bedterm_core/Cargo.toml`:

```
src/bin/bedterm-render/
├── main.rs      # clap argument parsing + dispatch
├── grid.rs      # stdin/fixture → bt_term_feed → Renderer::draw → PNG
├── blocks.rs    # SQLite / pipe → layout → Renderer::draw_block_list → PNG
├── layout.rs    # block layout logic ported from Swift BlockListContainerView
└── png.rs       # offscreen texture → get_bytes → image::png encode
```

No new crate. The binary links `bedterm_core` as a dependency and calls `Renderer`,
`BtTerm`, and `Block` types directly — already accessible within the crate.

## CLI surface

```
bedterm-render <mode> [options] [input]

Modes:
  grid        Classic terminal grid rendering
  blocks      Block list rendering (OSC 133 boundaries)

Common options:
  --font-size 14          Cell pixel height
  --viewport 1200x800     Canvas pixel dimensions
  --output out.png        Output file (default: stdout)
  --clear-color "0,0,0"   RGBA clear color, 0-255

Grid options:
  [input]                 stdin or .bin fixture file path

Block options:
  --db PATH               SQLite database (read existing session)
  --session-id ID         Session ID within the database
  --wrap-single-block     Wrap entire stdin as a single block
  --ui-scale 2.0          Header font scale (equivalent to UIScreen.scale)
  --header-font "15,12"   Subheadline + caption2 point sizes

Derived values:
  cell_h_px  = font_size
  cell_w_px  = font_size * 0.5 (monospace)
  cols       = viewport_w / cell_w_px
  rows       = viewport_h / cell_h_px
```

## Input sources

| Source | Mode | Use case |
|--------|------|----------|
| stdin pipe | grid | `echo hello \| cargo run bedterm-render grid -` |
| fixture .bin file | grid | `cargo run bedterm-render grid tests/fixtures/ls-color.bin` |
| stdin + OSC 133 | blocks | pipe from mock SSH or real shell with integration |
| stdin + `--wrap-single-block` | blocks | `ls --color \| cargo run bedterm-render blocks --wrap` |
| SQLite | blocks | `cargo run bedterm-render blocks --db sessions.sqlite --session-id 1` |

CLI never connects to a server. It only renders pre-recorded bytes. Recording happens
via the iOS app (persistence) or mock SSH (pipe). CLI exit is instant — no long-running
process, no SIGINT recovery needed.

## Layout porting

| Swift source | Rust destination | Content |
|---|---|---|
| `BlockListContainerView+Layout.swift` | `layout.rs` | `BlockRange`, `BtBlockLayoutEntry`, header descriptors |
| `BlockListContainerView+Sticky.swift` | `layout.rs` | sticky pinning + screen-space Y |
| `BlockListContainerView.swift` | `blocks.rs` | `computeBlockRanges`, `updateContentSize`, `bodyHeightPt` |

Color tokens (`ShadcnPrimary`, `ShadcnMutedForeground`, etc.) are hardcoded in a
built-in palette table keyed by palette name. No Xcode asset catalog dependency.

## Offscreen PNG pipeline

```rust
let device   = Device::system_default().unwrap();
let queue    = device.new_command_queue();
let tex_desc = TextureDescriptor::new();
tex_desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
tex_desc.set_width(width);
tex_desc.set_height(height);
tex_desc.set_storage_mode(MTLStorageMode::Managed); // macOS optimal
tex_desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
let tex = device.new_texture(&tex_desc);

// Call existing renderer — no pipeline changes
renderer.draw(term, tex_ptr, width, height, 0.0);

// Read back
let cmd = queue.new_command_buffer();
let blit = cmd.new_blit_command_encoder();
blit.synchronize_resource(&tex);
blit.end_encoding();
cmd.commit();
cmd.wait_until_completed();

let mut pixels = vec![0u8; (width * height * 4) as usize];
tex.get_bytes(&mut pixels, bytes_per_row, region, 0);

// PNG encode (image crate is already a dependency)
PngEncoder::new(stdout).encode(&pixels, width, height, ColorType::Rgba8::<u8>);
```

Zero renderer pipeline changes. `Renderer` only sees an `id<MTLTexture>` pointer — it
does not know or care whether the texture came from a `CAMetalDrawable` (iOS) or a
manually-allocated offscreen texture (macOS).

## Testing methodology

### When changing Rust rendering code
```sh
cargo run -p bedterm_core --bin bedterm-render grid tests/fixtures/<name>.bin > /tmp/out.png
```
Compare against golden PNG. No simulator needed.

### When changing block list layout
```sh
cargo run -p bedterm_mock_ssh -- --script tests/fixtures/multi-block.json &
ssh localhost -p 2222 "cmd1; cmd2; cmd3" 2>&1 | cargo run -p bedterm_core --bin bedterm-render blocks - > /tmp/out.png
```

### When changing Swift UIKit code
Must run on iOS. Use `worktree-ios-dev-tool test` and optionally `worktree-ios-dev-tool run` for manual verification.

### Fixture conventions
- `tests/fixtures/*.bin` — raw byte streams (may include OSC 133) for regression input
- `tests/fixtures/*.json` — canned session data for block list fixture tests
- `tests/fixtures/*.golden.png` — golden output images for comparison
- `tests/fixtures/multi-block.json` — mock SSH scripts defining multi-command sequences

### Agent-driven workflow
1. After any Rust renderer change: run `cargo build` on the CLI, render a fixture, check output
2. After layout port changes: render the same fixture against golden, diff
3. CI should eventually run `cargo test` for the CLI binary comparing against golden PNGs
