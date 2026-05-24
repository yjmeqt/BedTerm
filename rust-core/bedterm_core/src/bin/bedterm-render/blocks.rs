//! Block list mode: stdin → Terminal/FfiTerm → layout → draw_block_list → PNG.
//!
//! Feeds the same byte stream to both a Rust `Terminal` (for metadata extraction)
//! and an FFI `BtTerm` (for cell resolution in draw_block_list).  Block IDs are
//! deterministic so the renderer's `block_id`-based lookup finds matching cells.

use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::{self, Read};

use metal::foreign_types::ForeignType;
use metal::Device;

use bedterm_core::ffi::BtPaletteView;
use bedterm_core::renderer::block_list_ffi::bt_renderer_draw_block_list;
use bedterm_core::renderer::Renderer;
use bedterm_core::term::{BtRgb24, Palette, Terminal};

use crate::context::RenderContext;
use crate::layout::{self, TOKYO_NIGHT};
use crate::png::{self, OffscreenTarget};

pub(crate) struct BlockArgs {
    pub ui_scale: f32,
    pub wrap_single_block: bool,
    pub wrap_command: Option<String>,
    pub wrap_exit_code: i32,
    pub wrap_duration_ms: Option<u64>,
    pub input: Option<String>,
    /// Explicit --palette flag overrides context.
    pub palette_override: Option<Palette>,
}

impl Default for BlockArgs {
    fn default() -> Self {
        Self {
            ui_scale: 2.0,
            wrap_single_block: false,
            wrap_command: None,
            wrap_exit_code: 0,
            wrap_duration_ms: None,
            input: None,
            palette_override: None,
        }
    }
}

fn resolve_palette(ctx: &RenderContext, override_: Option<&Palette>) -> Palette {
    if let Some(p) = override_ {
        return *p;
    }
    match &ctx.palette {
        Some(name) => PalettePreset::from_str(name)
            .map(|p| p.build())
            .unwrap_or_default(),
        None => Palette::default(),
    }
}

// ── Palette presets ────────────────────────────────────────────────────

pub(crate) enum PalettePreset {
    Default,
    TokyoNight,
    SolarizedDark,
    SolarizedLight,
    Dracula,
    GruvboxDark,
}

impl PalettePreset {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "default" => Some(Self::Default),
            "tokyo-night" => Some(Self::TokyoNight),
            "solarized-dark" => Some(Self::SolarizedDark),
            "solarized-light" => Some(Self::SolarizedLight),
            "dracula" => Some(Self::Dracula),
            "gruvbox-dark" => Some(Self::GruvboxDark),
            _ => None,
        }
    }

    pub fn build(&self) -> Palette {
        match self {
            Self::Default => Palette::default(),
            Self::TokyoNight => Self::palette_from_hexs(
                (0xc0, 0xca, 0xf5),
                (0x24, 0x28, 0x3b),
                [
                    0x1a1b26, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xa9b1d6,
                    0x414868, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xc0caf5,
                ],
            ),
            Self::SolarizedDark => Self::palette_from_hexs(
                (0x83, 0x94, 0x96),
                (0x00, 0x2b, 0x36),
                [
                    0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
                    0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
                ],
            ),
            Self::SolarizedLight => Self::palette_from_hexs(
                (0x65, 0x7b, 0x83),
                (0xfd, 0xf6, 0xe3),
                [
                    0xeee8d5, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0x073642,
                    0xfdf6e3, 0xcb4b16, 0x93a1a1, 0x839496, 0x657b83, 0x6c71c4, 0x586e75, 0x002b36,
                ],
            ),
            Self::Dracula => Self::palette_from_hexs(
                (0xf8, 0xf8, 0xf2),
                (0x28, 0x2a, 0x36),
                [
                    0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
                    0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
                ],
            ),
            Self::GruvboxDark => Self::palette_from_hexs(
                (0xeb, 0xdb, 0xb2),
                (0x28, 0x28, 0x28),
                [
                    0x282828, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0xa89984,
                    0x928374, 0xfb4934, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xebdbb2,
                ],
            ),
        }
    }

    fn palette_from_hexs(fg: (u8, u8, u8), bg: (u8, u8, u8), ansi_hex: [u32; 16]) -> Palette {
        Palette {
            default_fg: BtRgb24 {
                r: fg.0,
                g: fg.1,
                b: fg.2,
            },
            default_bg: BtRgb24 {
                r: bg.0,
                g: bg.1,
                b: bg.2,
            },
            ansi: std::array::from_fn(|i| {
                let h = ansi_hex[i];
                BtRgb24 {
                    r: ((h >> 16) & 0xff) as u8,
                    g: ((h >> 8) & 0xff) as u8,
                    b: (h & 0xff) as u8,
                }
            }),
        }
    }
}

// ── DCS protocol helpers ───────────────────────────────────────────────

/// Build a DCS-wrapped shell-integration event.
/// Format: `ESC P $ d <hex-encoded JSON> 0x9C`
fn dcs_event(json: &str) -> Vec<u8> {
    let hex: String = json.bytes().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    });
    let mut v = vec![0x1B, b'P', b'$', b'd'];
    v.extend_from_slice(hex.as_bytes());
    v.push(0x9C);
    v
}

fn precmd_bytes() -> Vec<u8> {
    dcs_event(r#"{"hook":"Precmd","value":{}}"#)
}

fn preexec_bytes(cmd: &str) -> Vec<u8> {
    let json = format!(
        r#"{{"hook":"Preexec","value":{{"command":"{}"}}}}"#,
        json_escape(cmd)
    );
    dcs_event(&json)
}

fn command_finished_bytes(exit_code: i32) -> Vec<u8> {
    let json = format!(
        r#"{{"hook":"CommandFinished","value":{{"exit_code":{}}}}}"#,
        exit_code
    );
    dcs_event(&json)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Wrap raw output bytes in DCS shell-integration events so the block
/// store creates a single sealed block with the given metadata.
fn wrap_with_dcs(user_bytes: &[u8], args: &BlockArgs) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&precmd_bytes());
    let cmd = args.wrap_command.as_deref().unwrap_or("unknown");
    out.extend_from_slice(&preexec_bytes(cmd));
    out.extend_from_slice(user_bytes);
    out.extend_from_slice(&command_finished_bytes(args.wrap_exit_code));
    out
}

// ── Palette → FFI ──────────────────────────────────────────────────────

fn push_palette_to_ffi(ffi_term: *mut bedterm_core::ffi::BtTerm, palette: &Palette) {
    let view = BtPaletteView {
        default_fg: palette.default_fg,
        default_bg: palette.default_bg,
        ansi: palette.ansi,
    };
    unsafe {
        bedterm_core::ffi::bt_term_set_palette(ffi_term, &view);
    }
}

// ── Main entry point ───────────────────────────────────────────────────

pub(crate) fn run(args: BlockArgs, ctx: &RenderContext) -> Result<(), Box<dyn std::error::Error>> {
    let device = Device::system_default().ok_or("no Metal device found (must run on macOS)")?;
    let queue = device.new_command_queue();

    let mut renderer = unsafe {
        Renderer::from_ptrs(
            device.as_ptr() as *const std::ffi::c_void,
            queue.as_ptr() as *const std::ffi::c_void,
        )
    }
    .ok_or("failed to create renderer")?;

    crate::font::register_system_font();
    renderer.set_font(ctx.font_size_pt, ctx.scale);

    // Derive terminal geometry from actual font metrics.
    let (cell_w_px, cell_h_px) = renderer.cell_pixel_size();
    let cell_w = cell_w_px as f32;
    let cell_h = cell_h_px as f32;
    let (vp_w, vp_h) = ctx.viewport_px();
    let cols = ctx
        .cols
        .unwrap_or_else(|| (vp_w as f32 / cell_w).max(1.0) as u16);
    let rows = ctx
        .rows
        .unwrap_or_else(|| (vp_h as f32 / cell_h).max(1.0) as u16);

    let palette = resolve_palette(ctx, args.palette_override.as_ref());
    let bg = palette.default_bg;
    let clear = [
        bg.r as f32 / 255.0,
        bg.g as f32 / 255.0,
        bg.b as f32 / 255.0,
        1.0,
    ];
    renderer.set_clear_color(clear[0], clear[1], clear[2], clear[3]);
    renderer.set_ui_font_sizes(15.0 * args.ui_scale, 12.0 * args.ui_scale, args.ui_scale);

    let user_bytes: Vec<u8> = match &args.input {
        Some(path) => fs::read(path)?,
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };

    let stream_bytes = if args.wrap_single_block {
        wrap_with_dcs(&user_bytes, &args)
    } else {
        user_bytes
    };

    // Rust Terminal for metadata + layout.
    let mut terminal = Terminal::new(cols, rows);
    terminal.set_palette(palette);
    terminal.feed(&stream_bytes);

    let blocks = terminal.blocks();
    if blocks.is_empty() {
        let target = OffscreenTarget::new(&device, &queue, vp_w, vp_h)
            .ok_or("failed to create offscreen texture")?;
        let pixels = target.read_pixels();
        let stdout = io::stdout();
        return png::write_png(&mut stdout.lock(), &pixels, vp_w, vp_h);
    }

    let ui_scale = args.ui_scale;
    let width_px = vp_w as f32;

    let row_height_pt = ctx.font_size_pt / ui_scale;
    let ranges = layout::compute_block_ranges(blocks, row_height_pt);
    let layout_entries = layout::build_layout_entries(&ranges, ui_scale, width_px);
    let (mut headers, storage) =
        layout::build_header_descriptors(&ranges, ui_scale, width_px, &TOKYO_NIGHT);
    let _blob = layout::patch_header_pointers(&mut headers, &storage);

    // FFI term: replay the SAME bytes so block IDs are identical.
    let ffi_term = bedterm_core::ffi::bt_term_new(cols, rows);
    push_palette_to_ffi(ffi_term, &palette);
    unsafe {
        bedterm_core::ffi::bt_term_feed(ffi_term, stream_bytes.as_ptr(), stream_bytes.len());
    }

    let target = OffscreenTarget::new(&device, &queue, vp_w, vp_h)
        .ok_or("failed to create offscreen texture")?;

    unsafe {
        let ret = bt_renderer_draw_block_list(
            &mut renderer as *mut Renderer as *mut bedterm_core::renderer::ffi::BtRenderer,
            ffi_term,
            target.texture_ptr(),
            vp_w,
            vp_h,
            0.0,
            layout_entries.as_ptr(),
            layout_entries.len(),
            headers.as_ptr(),
            headers.len(),
        );
        if ret != 0 {
            bedterm_core::ffi::bt_term_free(ffi_term);
            return Err("bt_renderer_draw_block_list failed".into());
        }
        bedterm_core::ffi::bt_term_free(ffi_term);
    }

    let pixels = target.read_pixels();
    let stdout = io::stdout();
    png::write_png(&mut stdout.lock(), &pixels, vp_w, vp_h)?;

    eprintln!(
        "[blocks] cols={cols} rows={rows} cell={cell_w_px}×{cell_h_px}px viewport={vp_w}×{vp_h}px → PNG {vp_w}×{vp_h}"
    );
    Ok(())
}
