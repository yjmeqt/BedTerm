//! Local-loopback mock SSH server for BedTerm sim testing.
//!
//! Spawns `$SHELL -l` (falling back to `/bin/zsh`) inside a real PTY and
//! proxies bytes both ways. Auth is a no-op — any username, any password,
//! any pubkey is accepted. Bound to `127.0.0.1` only; the loopback bind
//! is the moat. **Do not** change the bind address — that would expose a
//! passwordless shell to the network.
//!
//! Usage:
//!   cargo run -p bedterm-mock-ssh -- [--port 2222] [--device iphone17]
//!
//! Sim connects to:
//!   host:     127.0.0.1
//!   port:     2222
//!   user:     anything
//!   password: anything

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use russh::server::{Auth, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, CryptoVec, MethodSet};
use russh_keys::key::KeyPair;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(
    name = "bedterm-mock-ssh",
    about = "Local mock SSH server for BedTerm sim testing"
)]
struct Args {
    /// Listen port on 127.0.0.1.
    #[arg(long, default_value_t = 2222)]
    port: u16,

    /// Device preset for PTY dimensions (e.g. iphone17, ipad-pro13, mac).
    /// Defaults to 80×24 when omitted.
    #[arg(long)]
    device: Option<String>,

    /// Font size in points (default: 14).
    #[arg(long, default_value_t = 14.0)]
    font_size: f32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Persistent host key so the iOS app's stored fingerprint stays valid
    // across mock-ssh restarts. Path: `$HOME/.cache/bedterm-mock-ssh/
    // host_ed25519.pem`. Generated on first run, then reused. Delete the
    // file to force a fresh key.
    let host_key = load_or_create_host_key()?;

    let config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        auth_rejection_time: Duration::from_secs(0),
        auth_rejection_time_initial: Some(Duration::from_secs(0)),
        keys: vec![host_key],
        methods: MethodSet::PASSWORD | MethodSet::PUBLICKEY,
        ..Default::default()
    });

    // Derive PTY dimensions from device preset + real font metrics.
    let (pty_cols, pty_rows) = derive_pty_dims(&args);

    let addr = ("127.0.0.1", args.port);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    eprintln!("[bedterm-mock-ssh] listening on 127.0.0.1:{}", args.port);
    eprintln!("[bedterm-mock-ssh] accepts any user + password");
    eprintln!("[bedterm-mock-ssh] PTY: {pty_cols}×{pty_rows}");
    eprintln!("[bedterm-mock-ssh] bridging to real PTY: {shell} -l");

    let mut srv = MockServer { pty_cols, pty_rows };
    srv.run_on_address(config, addr).await?;
    Ok(())
}

/// Derive terminal cols/rows from --device + --font-size using the same
/// font metrics the renderer uses. Falls back to 80×24 when no --device.
fn derive_pty_dims(args: &Args) -> (u16, u16) {
    let Some(ref device_name) = args.device else {
        return (80, 24);
    };
    let Some(device) = bedterm_core::device_presets::find_device(device_name) else {
        eprintln!(
            "[bedterm-mock-ssh] unknown device '{}', using 80×24",
            device_name
        );
        return (80, 24);
    };
    let vp_px_w = (device.viewport_pt.0 as f32 * device.scale).round() as u32;
    let vp_px_h = (device.viewport_pt.1 as f32 * device.scale).round() as u32;
    let mut rasterizer = bedterm_core::renderer::glyph_raster::SwashRasterizer::new();
    let Some(metrics) = bedterm_core::renderer::glyph_raster::GlyphRasterizer::measure_cell(
        &mut rasterizer,
        args.font_size,
        device.scale,
    ) else {
        eprintln!(
            "[bedterm-mock-ssh] cannot measure font at {}pt @{}x, using 80×24",
            args.font_size, device.scale
        );
        return (80, 24);
    };
    let cols = (vp_px_w as f32 / metrics.cell_width as f32).max(1.0) as u16;
    let rows = (vp_px_h as f32 / metrics.cell_height as f32).max(1.0) as u16;
    eprintln!(
        "[bedterm-mock-ssh] device={} viewport={}×{}pt @{}x → {}×{}px, cell={}×{}px → cols={}, rows={}",
        device_name,
        device.viewport_pt.0,
        device.viewport_pt.1,
        device.scale,
        vp_px_w,
        vp_px_h,
        metrics.cell_width,
        metrics.cell_height,
        cols,
        rows,
    );
    (cols, rows)
}

/// Path to the persisted ed25519 host key. Resolves under
/// `$HOME/.cache/bedterm-mock-ssh/` so it survives mock-ssh restarts and
/// keeps the iOS app's stored host fingerprint valid.
fn host_key_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("$HOME not set"))?;
    Ok(PathBuf::from(home)
        .join(".cache")
        .join("bedterm-mock-ssh")
        .join("host_ed25519.pem"))
}

fn load_or_create_host_key() -> Result<KeyPair> {
    let path = host_key_path()?;
    if let Ok(pem) = std::fs::read_to_string(&path) {
        match russh_keys::decode_secret_key(&pem, None) {
            Ok(key) => {
                eprintln!("[bedterm-mock-ssh] loaded host key from {}", path.display());
                return Ok(key);
            }
            Err(err) => {
                eprintln!(
                    "[bedterm-mock-ssh] stale host key at {} ({err:?}); regenerating",
                    path.display()
                );
            }
        }
    }

    let key = KeyPair::generate_ed25519()
        .ok_or_else(|| anyhow::anyhow!("ed25519 host-key generation failed"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut buf = Vec::new();
    russh_keys::encode_pkcs8_pem(&key, &mut buf)?;
    std::fs::write(&path, &buf)?;
    // 0o600 — host key is sensitive enough to keep user-only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    eprintln!(
        "[bedterm-mock-ssh] new host key persisted to {}",
        path.display()
    );
    Ok(key)
}

#[derive(Clone)]
struct MockServer {
    pty_cols: u16,
    pty_rows: u16,
}

impl Server for MockServer {
    type Handler = MockHandler;
    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> MockHandler {
        MockHandler {
            shell_tx: None,
            pty_cols: self.pty_cols,
            pty_rows: self.pty_rows,
        }
    }
}

enum ToShell {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

struct MockHandler {
    /// Control channel into the PTY bridge thread.
    shell_tx: Option<mpsc::UnboundedSender<ToShell>>,
    pty_cols: u16,
    pty_rows: u16,
}

#[async_trait]
impl Handler for MockHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _public_key: &russh_keys::key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        _term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Remember requested geometry — applied when the shell spawns.
        self.pty_cols = col_width.clamp(1, u16::MAX as u32) as u16;
        self.pty_rows = row_height.clamp(1, u16::MAX as u32) as u16;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.pty_cols = col_width.clamp(1, u16::MAX as u32) as u16;
        self.pty_rows = row_height.clamp(1, u16::MAX as u32) as u16;
        if let Some(tx) = &self.shell_tx {
            let _ = tx.send(ToShell::Resize {
                cols: self.pty_cols,
                rows: self.pty_rows,
            });
        }
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.shell_tx.is_some() {
            return Ok(());
        }
        let handle = session.handle();
        let tx = spawn_pty_bridge(handle, channel, self.pty_cols, self.pty_rows);
        self.shell_tx = Some(tx);
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(tx) = &self.shell_tx {
            let _ = tx.send(ToShell::Input(data.to_vec()));
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(tx) = self.shell_tx.take() {
            let _ = tx.send(ToShell::Close);
        }
        Ok(())
    }
}

/// Spawn a dedicated OS thread that owns the PTY master + child shell.
/// Returns a control channel; the thread exits when the child dies or
/// `ToShell::Close` is received.
fn spawn_pty_bridge(
    handle: russh::server::Handle,
    channel: ChannelId,
    cols: u16,
    rows: u16,
) -> mpsc::UnboundedSender<ToShell> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ToShell>();
    let rt = tokio::runtime::Handle::current();

    std::thread::Builder::new()
        .name("pty-bridge".into())
        .spawn(move || {
            let pty_sys = native_pty_system();
            let pair = match pty_sys.openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            }) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[bedterm-mock-ssh] openpty failed: {e:?}");
                    return;
                }
            };

            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            let mut cmd = CommandBuilder::new(&shell);
            cmd.arg("-l");
            cmd.env("TERM", "xterm-256color");
            cmd.env("LANG", "en_US.UTF-8");
            if let Some(home) = std::env::var_os("HOME") {
                cmd.cwd(home);
            }

            let mut child = match pair.slave.spawn_command(cmd) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[bedterm-mock-ssh] spawn shell failed: {e:?}");
                    return;
                }
            };
            drop(pair.slave);

            let mut reader = match pair.master.try_clone_reader() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[bedterm-mock-ssh] try_clone_reader: {e:?}");
                    let _ = child.kill();
                    return;
                }
            };
            let mut writer = match pair.master.take_writer() {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("[bedterm-mock-ssh] take_writer: {e:?}");
                    let _ = child.kill();
                    return;
                }
            };
            let master = pair.master;

            // Reader: PTY → SSH channel.
            let h_read = handle.clone();
            let rt_read = rt.clone();
            std::thread::Builder::new()
                .name("pty-reader".into())
                .spawn(move || {
                    let mut buf = [0u8; 8192];
                    loop {
                        match reader.read(&mut buf) {
                            Ok(0) => break,
                            Err(_) => break,
                            Ok(n) => {
                                let bytes = CryptoVec::from_slice(&buf[..n]);
                                let h = h_read.clone();
                                rt_read.block_on(async move {
                                    let _ = h.data(channel, bytes).await;
                                });
                            }
                        }
                    }
                })
                .expect("spawn pty-reader thread");

            // Control loop: writes, resizes, child-exit polling.
            loop {
                if let Ok(Some(status)) = child.try_wait() {
                    let code = status.exit_code();
                    rt.block_on(async {
                        let _ = handle.exit_status_request(channel, code).await;
                        let _ = handle.eof(channel).await;
                        let _ = handle.close(channel).await;
                    });
                    return;
                }
                let msg = rt.block_on(async {
                    tokio::time::timeout(Duration::from_millis(500), rx.recv()).await
                });
                match msg {
                    Ok(Some(ToShell::Input(b))) => {
                        let _ = writer.write_all(&b);
                        let _ = writer.flush();
                    }
                    Ok(Some(ToShell::Resize { cols, rows })) => {
                        let _ = master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                    Ok(Some(ToShell::Close)) | Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        rt.block_on(async {
                            let _ = handle.close(channel).await;
                        });
                        return;
                    }
                    Err(_) => continue,
                }
            }
        })
        .expect("spawn pty-bridge thread");

    tx
}
