//! `bedterm-render` — offline macOS CLI for rendering terminal frames to PNG.
//!
//! ```text
//! # Grid mode — classic terminal
//! echo "Hello World" | bedterm-render grid --font-size 14 > out.png
//!
//! # Block list mode — wrap stdin as a single block
//! ls --color=always | bedterm-render blocks --wrap --font-size 14 > out.png
//!
//! # Block list from SQLite
//! bedterm-render blocks --db sessions.sqlite --session-id abc123 > out.png
//! ```

mod blocks;
mod grid;
mod layout;
mod png;

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: bedterm-render <grid|blocks> [options]");
        eprintln!();
        eprintln!("Modes:");
        eprintln!("  grid     Classic terminal grid rendering (stdin byte stream)");
        eprintln!("  blocks   Block list rendering (OSC 133 boundaries)");
        eprintln!();
        eprintln!("Grid options:");
        eprintln!("  --font-size N     Cell pixel height (default: 14)");
        eprintln!("  --viewport WxH    Canvas pixels (default: 1200x800)");
        eprintln!("  --clear-color R,G,B  RGB clear color 0-255 (default: 0,0,0)");
        eprintln!("  [input]           File path, or omit for stdin");
        eprintln!();
        eprintln!("Block options:");
        eprintln!("  --font-size N     Cell pixel height (default: 14)");
        eprintln!("  --viewport WxH    Canvas pixels (default: 1200x800)");
        eprintln!("  --clear-color R,G,B  RGB clear color (default: 0,0,0)");
        eprintln!("  --ui-scale N      Header font scale (default: 2.0)");
        eprintln!("  --wrap            Wrap stdin as a single block");
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
            eprintln!("usage: bedterm-render <grid|blocks>");
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

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
            "--clear-color" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    opts.clear_color = parse_rgb(v);
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
            "--clear-color" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    opts.clear_color = parse_rgb(v);
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

fn parse_dims(s: &str) -> Option<(u32, u32)> {
    let mut parts = s.split('x');
    let w = parts.next()?.parse().ok()?;
    let h = parts.next()?.parse().ok()?;
    Some((w, h))
}

fn parse_rgb(s: &str) -> [f32; 4] {
    let mut parts = s.split(',');
    let r: f32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0) / 255.0;
    let g: f32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0) / 255.0;
    let b: f32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0) / 255.0;
    [r, g, b, 1.0]
}
