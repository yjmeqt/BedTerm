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
//! ```

mod blocks;
mod font;
mod grid;
mod layout;
mod png;

use std::env;
use std::process;

use blocks::PalettePreset;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let mode = &args[1];
    let result = match mode.as_str() {
        "grid" => {
            let opts = parse_grid_args(&args[2..]);
            grid::run(opts)
        }
        "blocks" => {
            let opts = parse_block_args(&args[2..]);
            blocks::run(opts)
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
    eprintln!("usage: bedterm-render <grid|blocks> [options]");
    eprintln!();
    eprintln!("Common options:");
    eprintln!("  --font-size N       Cell pixel height (default: 14)");
    eprintln!("  --viewport WxH      Canvas pixels (default: 1200x800)");
    eprintln!("  --palette NAME      Color palette preset (default: default)");
    eprintln!("                        Presets: default, tokyo-night, solarized-dark,");
    eprintln!("                        solarized-light, dracula, gruvbox-dark");
    eprintln!("  [input]             File path, or omit for stdin");
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
}

// ── Grid arg parsing ───────────────────────────────────────────────────

fn parse_grid_args(args: &[String]) -> grid::GridArgs {
    let mut opts = grid::GridArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--font-size" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse().ok()) {
                    opts.font_size = v;
                }
            }
            "--viewport" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    if let Some((w, h)) = parse_dims(v) {
                        opts.viewport_w = w;
                        opts.viewport_h = h;
                    }
                }
            }
            "--palette" => {
                i += 1;
                if let Some(p) = args.get(i).and_then(|s| PalettePreset::from_str(s)) {
                    opts.palette = p.build();
                }
            }
            other if !other.starts_with("--") => {
                opts.input = Some(other.to_string());
            }
            _ => {
                eprintln!("unknown flag: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }
    opts
}

// ── Block arg parsing ──────────────────────────────────────────────────

fn parse_block_args(args: &[String]) -> blocks::BlockArgs {
    let mut opts = blocks::BlockArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--font-size" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse().ok()) {
                    opts.font_size = v;
                }
            }
            "--viewport" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    if let Some((w, h)) = parse_dims(v) {
                        opts.viewport_w = w;
                        opts.viewport_h = h;
                    }
                }
            }
            "--palette" => {
                i += 1;
                if let Some(p) = args.get(i).and_then(|s| PalettePreset::from_str(s)) {
                    opts.palette = p.build();
                }
            }
            "--ui-scale" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse().ok()) {
                    opts.ui_scale = v;
                }
            }
            "--wrap" => {
                opts.wrap_single_block = true;
            }
            "--command" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    opts.wrap_command = Some(v.clone());
                }
            }
            "--exit-code" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse().ok()) {
                    opts.wrap_exit_code = v;
                }
            }
            "--duration-ms" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse().ok()) {
                    opts.wrap_duration_ms = Some(v);
                }
            }
            other if !other.starts_with("--") => {
                opts.input = Some(other.to_string());
            }
            _ => {
                eprintln!("unknown flag: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
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
