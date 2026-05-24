//! `bedterm-render` — offline macOS CLI for rendering terminal frames to PNG.
//!
//! ```text
//! # Grid mode — classic terminal
//! echo "Hello World" | bedterm-render grid > out.png
//!
//! # Block list mode — wrap stdin as a single block
//! ls --color | bedterm-render blocks --wrap --command "ls --color" --exit-code 0 > out.png
//!
//! # Multi-block from raw OSC 133 stream
//! cat multi-block.bin | bedterm-render blocks --palette tokyo-night > out.png
//!
//! # With device context (viewport in pts, auto-scale)
//! bedterm-render blocks --device iphone17 --context session.meta.json < session.bin > out.png
//! ```

mod blocks;
mod context;
mod font;
mod grid;
mod layout;
mod png;

use std::env;
use std::process;

use blocks::PalettePreset;
use context::RenderContext;
use metal::foreign_types::ForeignType;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let mode = &args[1];
    if mode == "cell-size" {
        let mut font_size: f32 = 14.0;
        let mut scale: f32 = 2.0;
        let mut i = 2;
        while i < args.len() {
            match args[i].as_str() {
                "--font-size" if i + 1 < args.len() => {
                    font_size = args[i + 1].parse().unwrap_or(14.0);
                    i += 2;
                }
                "--scale" if i + 1 < args.len() => {
                    scale = args[i + 1].parse().unwrap_or(2.0);
                    i += 2;
                }
                _ => i += 1,
            }
        }
        print_cell_size(font_size, scale);
        return;
    }

    // Build RenderContext from flags.
    let ctx = build_context(&args[2..]);

    let result = match mode.as_str() {
        "grid" => {
            let opts = parse_grid_args(&args[2..]);
            grid::run(opts, &ctx)
        }
        "blocks" => {
            let opts = parse_block_args(&args[2..]);
            blocks::run(opts, &ctx)
        }
        _ => {
            eprintln!("unknown mode: {mode}");
            print_usage();
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn print_usage() {
    eprintln!("usage: bedterm-render <grid|blocks|cell-size> [options]");
    eprintln!();
    eprintln!("Context options (shared between record and render):");
    eprintln!("  --device NAME      Device preset: iphone17, iphone17-promax,");
    eprintln!("                     ipad-mini, ipad-pro13, mac");
    eprintln!("  --scale N          Device-pixel ratio (default: 2.0, or from --device)");
    eprintln!("  --viewport WxH     Logical viewport in points (not pixels!)");
    eprintln!("                     Texture = viewport × scale pixels");
    eprintln!("  --font-size N      Font size in points (default: 14)");
    eprintln!("  --cols N           Explicit terminal columns (derived if omitted)");
    eprintln!("  --rows N           Explicit terminal rows");
    eprintln!("  --palette NAME     Color palette (default: bedterm-dark)");
    eprintln!("                       Presets: bedterm-dark, bedterm-light");
    eprintln!("  --context FILE     Load context from JSON sidecar (CLI flags override)");
    eprintln!("  [input]            File path, or omit for stdin");
    eprintln!();
    eprintln!("Grid mode (stdin byte stream → terminal frame):");
    eprintln!("  bedterm-render grid [options] [input]");
    eprintln!();
    eprintln!("Block mode (stdin byte stream → block list):");
    eprintln!("  bedterm-render blocks [options] [input]");
    eprintln!("  --ui-scale N        Header font scale (default: 2.0)");
    eprintln!("  --wrap              Wrap stdin as a single block");
    eprintln!("  --command TEXT      Command name for --wrap header");
    eprintln!("  --exit-code N       Exit code for --wrap (default: 0)");
    eprintln!("  --duration-ms N     Duration in ms for --wrap");
    eprintln!();
    eprintln!("Cell size query (respects --scale):");
    eprintln!("  bedterm-render cell-size --font-size 14 --scale 3.0");
    eprintln!("  Output: <cell_w_px> <cell_h_px>");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  # iPhone 17 rendering (400×850 pt @ 3x → 1200×2550 px PNG)");
    eprintln!("  cat session.bin | bedterm-render blocks --device iphone17 > out.png");
    eprintln!();
    eprintln!("  # Use context file from recording for faithful replay");
    eprintln!("  cat session.bin | bedterm-render blocks --context session.meta.json > out.png");
}

// ── Context builder ───────────────────────────────────────────────────────

fn build_context(extra: &[String]) -> RenderContext {
    let mut ctx = RenderContext::default();

    // --context FILE: load baseline, then CLI flags override.
    let mut i = 0;
    while i < extra.len() {
        if extra[i] == "--context" && i + 1 < extra.len() {
            match RenderContext::from_json_file(&extra[i + 1]) {
                Ok(loaded) => ctx = loaded,
                Err(e) => eprintln!("warning: {e}"),
            }
            break;
        }
        i += 1;
    }

    i = 0;
    while i < extra.len() {
        match extra[i].as_str() {
            "--device" if i + 1 < extra.len() => {
                if let Err(e) = ctx.apply_device(&extra[i + 1]) {
                    eprintln!("warning: {e}");
                }
                i += 2;
            }
            "--viewport" if i + 1 < extra.len() => {
                if let Some((w, h)) = parse_dims(&extra[i + 1]) {
                    ctx.viewport_pt = (w, h);
                }
                i += 2;
            }
            "--scale" if i + 1 < extra.len() => {
                ctx.scale = extra[i + 1].parse().unwrap_or(ctx.scale);
                i += 2;
            }
            "--font-size" if i + 1 < extra.len() => {
                ctx.font_size_pt = extra[i + 1].parse().unwrap_or(ctx.font_size_pt);
                i += 2;
            }
            "--cols" if i + 1 < extra.len() => {
                ctx.cols = extra[i + 1].parse().ok();
                i += 2;
            }
            "--rows" if i + 1 < extra.len() => {
                ctx.rows = extra[i + 1].parse().ok();
                i += 2;
            }
            "--palette" if i + 1 < extra.len() => {
                ctx.palette = Some(extra[i + 1].clone());
                i += 2;
            }
            // Skip flags parsed elsewhere.
            "--ui-scale" | "--command" | "--duration-ms" if i + 1 < extra.len() => {
                i += 2;
            }
            "--wrap" => {
                i += 1;
            }
            "--exit-code" => {
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    let (vpw, vph) = ctx.viewport_px();
    let cols_str = ctx.cols.map_or("auto".into(), |c| c.to_string());
    let rows_str = ctx.rows.map_or("auto".into(), |r| r.to_string());
    eprintln!(
        "[render] device-scale={} viewport={}×{}pt → {}×{}px font={}pt cols={} rows={}",
        ctx.scale,
        ctx.viewport_pt.0,
        ctx.viewport_pt.1,
        vpw,
        vph,
        ctx.font_size_pt,
        cols_str,
        rows_str,
    );

    ctx
}

fn print_cell_size(font_size: f32, scale: f32) {
    let device = match metal::Device::system_default() {
        Some(d) => d,
        None => {
            eprintln!("no Metal device");
            process::exit(1);
        }
    };
    let queue = device.new_command_queue();
    let mut renderer = unsafe {
        bedterm_core::renderer::Renderer::from_ptrs(
            device.as_ptr() as *const std::ffi::c_void,
            queue.as_ptr() as *const std::ffi::c_void,
        )
    };
    let Some(ref mut r) = renderer else {
        eprintln!("failed to create renderer");
        process::exit(1);
    };
    crate::font::register_system_font();
    r.set_font(font_size, scale);
    let (cw, ch) = r.cell_pixel_size();
    println!("{cw} {ch}");
    std::process::exit(0);
}

// ── Grid arg parsing ───────────────────────────────────────────────────

fn parse_grid_args(args: &[String]) -> grid::GridArgs {
    let mut opts = grid::GridArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--font-size" | "--viewport" | "--palette" | "--scale" | "--cols" | "--rows"
            | "--device" | "--context"
                if i + 1 < args.len() =>
            {
                i += 2; // handled by build_context / parse_block_args
            }
            "--font-size" | "--viewport" | "--palette" | "--scale" | "--cols" | "--rows"
            | "--device" | "--context" => {
                i += 1;
            }
            other if !other.starts_with("--") => {
                opts.input = Some(other.to_string());
                i += 1;
            }
            _ => {
                eprintln!("unknown flag: {}", args[i]);
                process::exit(1);
            }
        }
    }
    opts
}

// ── Block arg parsing ──────────────────────────────────────────────────

fn parse_block_args(args: &[String]) -> blocks::BlockArgs {
    let mut opts = blocks::BlockArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--font-size" | "--viewport" | "--scale" | "--cols" | "--rows" | "--device"
            | "--context"
                if i + 1 < args.len() =>
            {
                i += 2; // handled by build_context / palette from ctx
            }
            "--font-size" | "--viewport" | "--scale" | "--cols" | "--rows" | "--device"
            | "--context" => {
                i += 1;
            }
            "--palette" => {
                if let Some(p) = args.get(i + 1).and_then(|s| PalettePreset::from_str(s)) {
                    opts.palette_override = Some(p.build());
                }
                i += 2;
            }
            "--ui-scale" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    opts.ui_scale = v;
                }
                i += 2;
            }
            "--wrap" => {
                opts.wrap_single_block = true;
                i += 1;
            }
            "--command" => {
                if let Some(v) = args.get(i + 1) {
                    opts.wrap_command = Some(v.clone());
                }
                i += 2;
            }
            "--exit-code" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    opts.wrap_exit_code = v;
                }
                i += 2;
            }
            "--duration-ms" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    opts.wrap_duration_ms = Some(v);
                }
                i += 2;
            }
            other if !other.starts_with("--") => {
                opts.input = Some(other.to_string());
                i += 1;
            }
            _ => {
                eprintln!("unknown flag: {}", args[i]);
                process::exit(1);
            }
        }
    }
    opts
}

// ── Helpers ────────────────────────────────────────────────────────────

fn parse_dims(s: &str) -> Option<(u32, u32)> {
    let mut parts = s.split('x');
    let w = parts.next()?.parse().ok()?;
    let h = parts.next()?.parse().ok()?;
    Some((w, h))
}
