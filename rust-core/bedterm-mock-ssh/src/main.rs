//! Local-loopback mock SSH server for BedTerm sim testing.
//!
//! Two modes:
//!   * default (script): streams a canned OSC 133-framed transcript and
//!     ignores client input. Useful for renderer + block-state tests.
//!   * `--shell`: spawns `$SHELL -l` in a real PTY and proxies bytes
//!     both ways, so the sim gets an interactive zsh as the current
//!     macOS user with no password needed. Bound to `127.0.0.1` only.
//!
//! Usage:
//!   cargo run -p bedterm-mock-ssh -- [--port 2222] [--shell]
//!
//! Sim connects to:
//!   host:     127.0.0.1
//!   port:     2222
//!   user:     anything
//!   password: anything (only loopback binds — that is the moat)

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use russh::server::{Auth, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, CryptoVec, MethodSet};
use russh_keys::key::KeyPair;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(
    name = "bedterm-mock-ssh",
    about = "Local mock SSH server for BedTerm sim testing"
)]
struct Args {
    /// Listen port on 127.0.0.1.
    #[arg(long, default_value_t = 2222)]
    port: u16,

    /// Bridge to a real PTY running `$SHELL -l` instead of replaying
    /// the canned script. Loopback-only — safe for local sim testing.
    #[arg(long)]
    shell: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Fresh host key every run — no fingerprint persistence intentionally,
    // since each launch is a throwaway test session.
    let host_key = KeyPair::generate_ed25519()
        .ok_or_else(|| anyhow::anyhow!("ed25519 host-key generation failed"))?;

    let config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        auth_rejection_time: Duration::from_secs(0),
        auth_rejection_time_initial: Some(Duration::from_secs(0)),
        keys: vec![host_key],
        methods: MethodSet::PASSWORD | MethodSet::PUBLICKEY,
        ..Default::default()
    });

    let addr = ("127.0.0.1", args.port);
    eprintln!("[bedterm-mock-ssh] listening on 127.0.0.1:{}", args.port);
    eprintln!("[bedterm-mock-ssh] accepts any user + password");
    if args.shell {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        eprintln!("[bedterm-mock-ssh] --shell mode: bridging to {shell}");
    } else {
        eprintln!("[bedterm-mock-ssh] script mode (pass --shell for real PTY)");
    }

    let mut srv = MockServer {
        shell_mode: args.shell,
    };
    srv.run_on_address(config, addr).await?;
    Ok(())
}

#[derive(Clone)]
struct MockServer {
    shell_mode: bool,
}

impl Server for MockServer {
    type Handler = MockHandler;
    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> MockHandler {
        MockHandler {
            shell_mode: self.shell_mode,
            script_done: false,
            shell_tx: None,
            pty_cols: 80,
            pty_rows: 24,
        }
    }
}

enum ToShell {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

struct MockHandler {
    shell_mode: bool,
    /// Set once the canned script has been played for this connection.
    script_done: bool,
    /// Control channel into the PTY bridge thread (shell mode only).
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
        if self.shell_mode {
            if self.shell_tx.is_some() {
                return Ok(());
            }
            let handle = session.handle();
            let tx = spawn_pty_bridge(handle, channel, self.pty_cols, self.pty_rows);
            self.shell_tx = Some(tx);
            return Ok(());
        }
        if self.script_done {
            return Ok(());
        }
        self.script_done = true;
        let handle = session.handle();
        tokio::spawn(async move {
            if let Err(err) = play_script(&handle, channel).await {
                eprintln!("[bedterm-mock-ssh] script error: {err:?}");
            }
        });
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
        // Script mode: silently discard input. The canned transcript
        // doesn't model an interactive shell.
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

/// Send one canned terminal session: three OSC 133-framed commands,
/// each with simulated output, then an unfinished prompt at the end.
async fn play_script(handle: &russh::server::Handle, channel: ChannelId) -> Result<()> {
    macro_rules! send {
        ($bytes:expr) => {{
            let _ = handle.data(channel, CryptoVec::from_slice($bytes)).await;
        }};
    }

    // OSC 133 helpers.
    const OSC_A: &[u8] = b"\x1b]133;A\x1b\\";
    const OSC_B: &[u8] = b"\x1b]133;B\x1b\\";

    // 1) ls
    send!(OSC_A);
    send!(b"\x1b[1;32muser@bedterm-mock\x1b[0m:\x1b[1;34m~\x1b[0m$ ");
    send!(OSC_B);
    send!(b"\x1b]133;C;cmd=bHM=\x1b\\"); // cmd="ls"
    send!(b"ls\r\n");
    sleep(Duration::from_millis(80)).await;
    send!(b"Cargo.toml      README.md       src/\r\n");
    send!(b"Cargo.lock      examples/       target/\r\n");
    send!(b"\x1b]133;D;0\x1b\\");

    // 2) echo with CJK + emoji
    send!(OSC_A);
    send!(b"\x1b[1;32muser@bedterm-mock\x1b[0m:\x1b[1;34m~\x1b[0m$ ");
    send!(OSC_B);
    // cmd = `echo "你好 こんにちは 안녕 😀🎉"`
    send!(b"\x1b]133;C;cmd=ZWNobyAi5L2g5aW9IOOBk+OCk+OBq+OBoeOBryDslYjri4Eg8J+YgPCfjok=\x1b\\");
    send!(b"echo \"\xe4\xbd\xa0\xe5\xa5\xbd \xe3\x81\x93\xe3\x82\x93\xe3\x81\xab\xe3\x81\xa1\xe3\x81\xaf \xec\x95\x88\xeb\x85\x95 \xf0\x9f\x98\x80\xf0\x9f\x8e\x89\"\r\n");
    sleep(Duration::from_millis(60)).await;
    send!(b"\xe4\xbd\xa0\xe5\xa5\xbd \xe3\x81\x93\xe3\x82\x93\xe3\x81\xab\xe3\x81\xa1\xe3\x81\xaf \xec\x95\x88\xeb\x85\x95 \xf0\x9f\x98\x80\xf0\x9f\x8e\x89\r\n");
    send!(b"\x1b]133;D;0\x1b\\");

    // 3) `false` — a failing command to exercise the red status path.
    send!(OSC_A);
    send!(b"\x1b[1;32muser@bedterm-mock\x1b[0m:\x1b[1;34m~\x1b[0m$ ");
    send!(OSC_B);
    send!(b"\x1b]133;C;cmd=ZmFsc2U=\x1b\\"); // cmd="false"
    send!(b"false\r\n");
    sleep(Duration::from_millis(40)).await;
    send!(b"\x1b]133;D;1\x1b\\");

    // 4) Final prompt — left "running" so the pulse animation shows.
    send!(OSC_A);
    send!(b"\x1b[1;32muser@bedterm-mock\x1b[0m:\x1b[1;34m~\x1b[0m$ ");
    send!(OSC_B);
    // No OSC 133;C/D — last block stays running.
    Ok(())
}
