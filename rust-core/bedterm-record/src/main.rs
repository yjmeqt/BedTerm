//! `bedterm-record` — record a real terminal session via PTY to a `.bin` file
//! plus a `.meta.json` sidecar for faithful replay.
//!
//! ```text
//! # Record with device preset
//! bedterm-record --cmd "ls --color" --device iphone17 -o ls-session.bin
//!
//! # Record with explicit viewport (points) + scale
//! bedterm-record --cmd claude --stdin --device iphone17 -o claude-session.bin
//!
//! # Replay + render (uses .meta.json for consistent dimensions)
//! cat ls-session.bin | bedterm-render blocks --context ls-session.meta.json > out.png
//! ```

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

fn main() -> Result<()> {
    let args = parse_args();
    run(args)
}

struct Args {
    cmd: Option<String>,
    output: PathBuf,
    integration: PathBuf,
    cols: u16,
    rows: u16,
    idle_timeout_secs: u64,
    font_size_pt: f32,
    viewport_pt: Option<(u32, u32)>,
    scale: f32,
    stdin_lines: bool,
    palette: Option<String>,
}

fn parse_args() -> Args {
    let mut args = Args {
        cmd: None,
        output: PathBuf::from("session.bin"),
        integration: find_integration_script(),
        cols: 80,
        rows: 24,
        idle_timeout_secs: 5,
        font_size_pt: 14.0,
        viewport_pt: None,
        scale: 2.0,
        stdin_lines: false,
        palette: None,
    };

    let raw: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--cmd" => {
                i += 1;
                args.cmd = Some(raw[i].clone());
            }
            "--output" | "-o" => {
                i += 1;
                args.output = PathBuf::from(&raw[i]);
            }
            "--integration" => {
                i += 1;
                args.integration = PathBuf::from(&raw[i]);
            }
            "--cols" => {
                i += 1;
                args.cols = raw[i].parse().unwrap_or(80);
            }
            "--rows" => {
                i += 1;
                args.rows = raw[i].parse().unwrap_or(24);
            }
            "--font-size" => {
                i += 1;
                args.font_size_pt = raw[i].parse().unwrap_or(14.0);
            }
            "--device" => {
                i += 1;
                let name = raw[i].as_str();
                if let Some(d) = bedterm_core::device_presets::find_device(name) {
                    args.viewport_pt = Some(d.viewport_pt);
                    args.scale = d.scale;
                } else {
                    eprintln!("[record] unknown device '{name}', using defaults");
                }
            }
            "--scale" => {
                i += 1;
                args.scale = raw[i].parse().unwrap_or(2.0);
            }
            "--viewport" => {
                i += 1;
                if let Some((w, h)) = parse_dims(&raw[i]) {
                    args.viewport_pt = Some((w, h));
                }
            }
            "--palette" => {
                i += 1;
                args.palette = Some(raw[i].clone());
            }
            "--timeout" => {
                i += 1;
                args.idle_timeout_secs = raw[i].parse().unwrap_or(5);
            }
            "--stdin" => {
                args.stdin_lines = true;
            }
            _ => {
                eprintln!("unknown flag: {}", raw[i]);
                print_usage();
                std::process::exit(1);
            }
        }
        i += 1;
    }
    if args.cmd.is_none() {
        eprintln!("--cmd required");
        print_usage();
        std::process::exit(1);
    }

    // Derive cols/rows from viewport + actual font metrics.
    let cols_explicit = raw.iter().any(|a| a == "--cols");
    let rows_explicit = raw.iter().any(|a| a == "--rows");

    if !cols_explicit && !rows_explicit {
        if let Some((vpw, vph)) = args.viewport_pt {
            // Query exact cell metrics from bedterm-render so PTY cols/rows
            // match the renderer's actual font rasterization, not a heuristic.
            let vp_px_w = (vpw as f32 * args.scale) as u32;
            let vp_px_h = (vph as f32 * args.scale) as u32;
            let (cell_w, cell_h) =
                query_cell_size(args.font_size_pt, args.scale).unwrap_or_else(|| {
                    eprintln!(
                        "[record] ERROR: cannot query cell-size from bedterm-render. \
                         Build it first: cargo build -p bedterm_core --bin bedterm-render"
                    );
                    std::process::exit(1);
                });
            args.cols = (vp_px_w as f32 / cell_w as f32).max(1.0) as u16;
            args.rows = (vp_px_h as f32 / cell_h as f32).max(1.0) as u16;
            eprintln!(
                "[record] cell={cell_w}x{cell_h}px → viewport={vp_px_w}x{vp_px_h}px → cols={}, rows={}",
                args.cols, args.rows,
            );
        }
    } else {
        eprintln!(
            "[record] using explicit --cols={} --rows={}",
            args.cols, args.rows
        );
    }

    args
}

fn find_integration_script() -> PathBuf {
    for c in [
        "../BedTermKit/Sources/BedTermKit/ShellIntegrationResources/bedterm-integration.sh",
        "../../BedTermKit/Sources/BedTermKit/ShellIntegrationResources/bedterm-integration.sh",
    ] {
        let p = PathBuf::from(c);
        if p.exists() {
            return p.canonicalize().unwrap_or(p);
        }
    }
    PathBuf::from("bedterm-integration.sh")
}

fn print_usage() {
    eprintln!("usage: bedterm-record --cmd TEXT [options]\n");
    eprintln!("Options:");
    eprintln!("  --cmd TEXT        Command to run in the shell (required)");
    eprintln!("  -o, --output      Output .bin file (default: session.bin)");
    eprintln!("  --device NAME     Device preset: iphone17, iphone17-promax,");
    eprintln!("                    ipad-mini, ipad-pro13, mac");
    eprintln!("  --viewport WxH    Logical viewport in points (overrides --device)");
    eprintln!("  --scale N         Device-pixel ratio (default: 2.0, or from --device)");
    eprintln!("  --font-size N     Font size in points (default: 14)");
    eprintln!("  --cols N          Terminal columns (derived if omitted)");
    eprintln!("  --rows N          Terminal rows (derived if omitted)");
    eprintln!("  --palette NAME    Color palette preset for sidecar metadata");
    eprintln!("  --stdin           After the command, pipe stdin lines to PTY");
    eprintln!("  --timeout N       Idle timeout in seconds (default: 5)");
    eprintln!("\nExamples:");
    eprintln!("  bedterm-record --cmd \"ls --color\" --device iphone17 -o ls.bin");
    eprintln!("  printf 'prompt\\n/exit\\n' | bedterm-record --cmd claude --stdin --device iphone17 -o claude.bin");
}

fn parse_dims(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// Try to spawn `bedterm-render cell-size` to get exact font metrics.
/// Returns `Some((cell_w_px, cell_h_px))` on success.
fn query_cell_size(font_size_pt: f32, scale: f32) -> Option<(u32, u32)> {
    // Look for bedterm-render next to the current binary.
    let self_path = std::env::current_exe().ok()?;
    let bin_dir = self_path.parent()?;
    let render_bin = bin_dir.join("bedterm-render");
    let output = std::process::Command::new(&render_bin)
        .args([
            "cell-size",
            "--font-size",
            &font_size_pt.to_string(),
            "--scale",
            &scale.to_string(),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = std::str::from_utf8(&output.stdout).ok()?;
    let mut parts = stdout.split_whitespace();
    let w: u32 = parts.next()?.parse().ok()?;
    let h: u32 = parts.next()?.parse().ok()?;
    Some((w, h))
}

// ── Sidecar ───────────────────────────────────────────────────────────────

fn write_sidecar(bin_path: &std::path::Path, args: &Args) {
    let meta_path = bin_path.with_extension("meta.json");
    let cols = args.cols;
    let rows = args.rows;
    let palette = args
        .palette
        .as_ref()
        .map_or("null".to_string(), |v| format!("\"{v}\""));
    let (vpw, vph) = args.viewport_pt.unwrap_or((1200, 800));
    let json = format!(
        "{{\n  \"viewport_pt\": [{vpw}, {vph}],\n  \"scale\": {},\n  \"font_size_pt\": {},\n  \"cols\": {cols},\n  \"rows\": {rows},\n  \"palette\": {palette},\n  \"appearance\": null\n}}\n",
        args.scale,
        args.font_size_pt,
    );
    if let Err(e) = fs::write(&meta_path, &json) {
        eprintln!("[record] warning: could not write sidecar: {e}");
    } else {
        eprintln!("[record] sidecar: {}", meta_path.display());
    }
}

// ── PTY logic ─────────────────────────────────────────────────────────────

fn run(args: Args) -> Result<()> {
    let pty_system = NativePtySystem::default();

    let integration_script = fs::read_to_string(&args.integration).with_context(|| {
        format!(
            "integration script not found: {}",
            args.integration.display()
        )
    })?;

    // Write a temp .zshrc that sources the integration script + forces
    // terminal size. stty is needed because macOS doesn't send SIGWINCH
    // after ioctl(TIOCSWINSZ); the shell must explicitly configure the tty.
    let tmpdir = tempfile::tempdir().context("create temp dir for ZDOTDIR")?;
    let zshrc = tmpdir.path().join(".zshrc");
    let zshrc_content = format!("stty cols {} rows {} 2>/dev/null\n", args.cols, args.rows)
        + &integration_script
        + "\n";
    fs::write(&zshrc, &zshrc_content).context("write .zshrc")?;

    let pty_size = PtySize {
        rows: args.rows,
        cols: args.cols,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = pty_system.openpty(pty_size).context("failed to open PTY")?;

    let mut cmd = CommandBuilder::new("zsh");
    cmd.arg("-i");
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLUMNS", args.cols.to_string());
    cmd.env("LINES", args.rows.to_string());
    cmd.env("ZDOTDIR", tmpdir.path().to_string_lossy().to_string());

    eprintln!("[record] spawning zsh ({}x{})...", args.cols, args.rows);
    let mut _child = pair
        .slave
        .spawn_command(cmd)
        .context("failed to spawn zsh")?;

    // MUST resize after spawn: openpty sets the initial size, but the
    // shell / child may query TIOCGWINSZ before env vars are evaluated.
    // resize() writes the kernel winsize + sends SIGWINCH so TUIs
    // (claude, codex, vim) pick up the correct rows × cols.
    pair.master.resize(pty_size).context("resize PTY")?;

    let mut reader = pair.master.try_clone_reader().context("no PTY reader")?;
    let mut writer = pair.master.take_writer().context("no PTY writer")?;

    // Reader thread.
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Phase 1: wait for shell startup (integration sourced via .zshrc).
    eprintln!("[record] waiting for shell...");
    let startup = drain_channel(&rx, Duration::from_millis(1200));
    let mut captured = startup;
    if let Ok(s) = std::str::from_utf8(&captured) {
        if !s.is_empty() {
            eprint!("{s}");
        }
    }

    // Phase 2: send the command + optional stdin lines.
    let cmd_line = args.cmd.clone().unwrap();
    eprintln!("\n[record] running: {cmd_line}");

    if args.stdin_lines {
        writer
            .write_all(format!("{cmd_line}\n").as_bytes())
            .context("write cmd to PTY")?;

        let stdin_lines: Vec<String> = {
            BufReader::new(std::io::stdin())
                .lines()
                .map_while(Result::ok)
                .collect()
        };

        let exit_idx = stdin_lines
            .iter()
            .position(|l| l.trim() == "/exit" || l.trim() == "exit");
        let prompt_lines = &stdin_lines[..exit_idx.unwrap_or(stdin_lines.len())];
        let has_explicit_exit = exit_idx.is_some();

        let startup_out = drain_channel_until_idle(&rx, Duration::from_secs(2));
        captured.extend_from_slice(&startup_out);
        if let Ok(s) = std::str::from_utf8(&startup_out) {
            if !s.is_empty() {
                eprint!("{s}");
            }
        }

        for line in prompt_lines {
            std::thread::sleep(Duration::from_millis(300));
            eprintln!("\n[record] typing: {line}");
            writer
                .write_all(line.as_bytes())
                .context("write prompt to PTY")?;
            writer.write_all(b"\n").context("write newline to PTY")?;
        }

        let cmd_out =
            drain_channel_until_idle(&rx, Duration::from_secs(args.idle_timeout_secs.max(120)));
        captured.extend_from_slice(&cmd_out);
        if let Ok(s) = std::str::from_utf8(&cmd_out) {
            if !s.is_empty() {
                eprint!("{s}");
            }
        }

        let exit_cmd = if has_explicit_exit {
            "/exit\n"
        } else {
            "exit\n"
        };
        let _ = writer.write_all(exit_cmd.as_bytes());
        let final_out = drain_channel(&rx, Duration::from_millis(500));
        captured.extend_from_slice(&final_out);
        drop(writer);
    } else {
        writer
            .write_all(format!("{cmd_line}\n").as_bytes())
            .context("write cmd to PTY")?;
        let cmd_out = drain_channel_until_idle(&rx, Duration::from_secs(args.idle_timeout_secs));
        captured.extend_from_slice(&cmd_out);
        if let Ok(s) = std::str::from_utf8(&cmd_out) {
            if !s.is_empty() {
                eprint!("{s}");
            }
        }
        let _ = writer.write_all(b"exit\n");
        let final_out = drain_channel(&rx, Duration::from_millis(300));
        captured.extend_from_slice(&final_out);
        drop(writer);
    }
    drop(tmpdir);

    eprintln!("\n[record] captured {} bytes", captured.len());
    fs::write(&args.output, &captured)
        .with_context(|| format!("failed to write {}", args.output.display()))?;
    eprintln!("[record] saved to {}", args.output.display());

    // Write JSON sidecar for faithful replay.
    write_sidecar(&args.output, &args);

    eprintln!(
        "[record] replay: cat {} | bedterm-render blocks --context {}.meta.json > out.png",
        args.output.display(),
        args.output.with_extension("").display(),
    );
    Ok(())
}

// ── Channel drain helpers ─────────────────────────────────────────────────

fn drain_channel(rx: &mpsc::Receiver<Vec<u8>>, timeout: Duration) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        match rx.recv_timeout(timeout) {
            Ok(chunk) => out.extend_from_slice(&chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    out
}

fn drain_channel_until_idle(rx: &mpsc::Receiver<Vec<u8>>, idle: Duration) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        match rx.recv_timeout(idle) {
            Ok(chunk) => out.extend_from_slice(&chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                eprintln!("\n[record] idle for {:.1}s, stopping", idle.as_secs_f32());
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    out
}
