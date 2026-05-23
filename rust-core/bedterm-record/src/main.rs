//! `bedterm-record` — record a real terminal session via PTY to a `.bin` file.
//!
//! ```text
//! bedterm-record --cmd "ls --color" --font-size 14 --viewport 400x850 -o ls-session.bin
//! bedterm-record --cmd "claude -p 'fix the auth bug'" --font-size 14 --viewport 400x850 -o claude-session.bin
//!
//! # Replay + render
//! cat ls-session.bin | bedterm-render blocks --palette tokyo-night > out.png
//! ```

use std::fs;
use std::io::{Read, Write};
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
    font_size: Option<f32>,
    viewport_w: Option<u32>,
    viewport_h: Option<u32>,
}

fn parse_args() -> Args {
    let mut args = Args {
        cmd: None,
        output: PathBuf::from("session.bin"),
        integration: find_integration_script(),
        cols: 80,
        rows: 24,
        idle_timeout_secs: 5,
        font_size: None,
        viewport_w: None,
        viewport_h: None,
    };

    let raw: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--cmd" => { i += 1; args.cmd = Some(raw[i].clone()); }
            "--output" | "-o" => { i += 1; args.output = PathBuf::from(&raw[i]); }
            "--integration" => { i += 1; args.integration = PathBuf::from(&raw[i]); }
            "--cols" => { i += 1; args.cols = raw[i].parse().unwrap_or(80); }
            "--rows" => { i += 1; args.rows = raw[i].parse().unwrap_or(24); }
            "--font-size" => { i += 1; args.font_size = raw[i].parse().ok(); }
            "--viewport" => {
                i += 1;
                if let Some((w, h)) = parse_dims(&raw[i]) {
                    args.viewport_w = Some(w);
                    args.viewport_h = Some(h);
                }
            }
            "--timeout" => { i += 1; args.idle_timeout_secs = raw[i].parse().unwrap_or(5); }
            _ => { eprintln!("unknown flag: {}", raw[i]); print_usage(); std::process::exit(1); }
        }
        i += 1;
    }
    if args.cmd.is_none() { eprintln!("--cmd required"); print_usage(); std::process::exit(1); }

    if let (Some(fs), Some(vw), Some(vh)) = (args.font_size, args.viewport_w, args.viewport_h) {
        let cell_w = (fs * 0.55).max(1.0);
        let cell_h = (fs * 1.25).max(1.0);
        args.cols = (vw as f32 / cell_w).max(1.0) as u16;
        args.rows = (vh as f32 / cell_h).max(1.0) as u16;
        eprintln!(
            "[record] viewport={vw}x{vh} font={fs} → cols={}, rows={}",
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
        if p.exists() { return p.canonicalize().unwrap_or(p); }
    }
    PathBuf::from("bedterm-integration.sh")
}

fn print_usage() {
    eprintln!("usage: bedterm-record --cmd TEXT [options]\n");
    eprintln!("Options:");
    eprintln!("  --cmd TEXT        Command to run in the shell (required)");
    eprintln!("  -o, --output      Output .bin file (default: session.bin)");
    eprintln!("  --font-size N     Font pixel size — with --viewport, derives cols/rows from cell metrics");
    eprintln!("  --viewport WxH    Canvas pixels — with --font-size, computes cols/rows");
    eprintln!("  --cols N          Terminal columns (default: 80, or derived from viewport+font)");
    eprintln!("  --rows N          Terminal rows (default: 24, or derived from viewport+font)");
    eprintln!("  --timeout N       Idle timeout in seconds (default: 5)");
}

fn parse_dims(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn run(args: Args) -> Result<()> {
    let pty_system = NativePtySystem::default();

    let integration_script = fs::read_to_string(&args.integration)
        .with_context(|| format!("integration script not found: {}", args.integration.display()))?;

    // Write a temp .zshrc that sources the integration script + suppresses
    // the ZLE echo we'd get from sending `eval` via PTY write.
    let tmpdir = tempfile::tempdir().context("create temp dir for ZDOTDIR")?;
    let zshrc = tmpdir.path().join(".zshrc");
    fs::write(&zshrc, format!("{}\n", integration_script))
        .context("write .zshrc")?;

    let pty_size = PtySize { rows: args.rows, cols: args.cols, pixel_width: 0, pixel_height: 0 };
    let pair = pty_system.openpty(pty_size).context("failed to open PTY")?;

    let mut cmd = CommandBuilder::new("zsh");
    cmd.arg("-i");
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLUMNS", args.cols.to_string());
    cmd.env("LINES", args.rows.to_string());
    cmd.env("ZDOTDIR", tmpdir.path().to_string_lossy().to_string());

    eprintln!("[record] spawning zsh ({}x{})...", args.cols, args.rows);
    let mut _child = pair.slave.spawn_command(cmd).context("failed to spawn zsh")?;

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
                    if tx.send(buf[..n].to_vec()).is_err() { break; }
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
        if !s.is_empty() { eprint!("{s}"); }
    }

    // Phase 2: run the command.
    let cmd_line = args.cmd.unwrap();
    eprintln!("\n[record] running: {cmd_line}");
    writer.write_all(format!("{cmd_line}\n").as_bytes()).context("write cmd to PTY")?;

    let cmd_out = drain_channel_until_idle(&rx, Duration::from_secs(args.idle_timeout_secs));
    captured.extend_from_slice(&cmd_out);
    if let Ok(s) = std::str::from_utf8(&cmd_out) {
        if !s.is_empty() { eprint!("{s}"); }
    }

    // Phase 3: exit.
    let _ = writer.write_all(b"exit\n");
    let final_out = drain_channel(&rx, Duration::from_millis(300));
    captured.extend_from_slice(&final_out);
    drop(writer);
    drop(tmpdir);

    eprintln!("\n[record] captured {} bytes", captured.len());
    fs::write(&args.output, &captured)
        .with_context(|| format!("failed to write {}", args.output.display()))?;
    eprintln!("[record] saved to {}", args.output.display());
    eprintln!(
        "[record] replay: cat {} | bedterm-render blocks - > out.png",
        args.output.display()
    );
    Ok(())
}

/// Drain the channel for up to `timeout`, returning all received chunks.
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

/// Drain channel, resetting the timer on each chunk. Stops when no
/// chunk arrives for `idle` duration.
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

