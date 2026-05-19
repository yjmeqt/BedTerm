//! Local-loopback mock SSH server for BedTerm sim testing.
//!
//! Spins up a russh-backed SSH server on 127.0.0.1:<port>, accepts any
//! username + password, and on shell request streams a scripted session
//! of OSC 133-framed prompts + commands + output. The iOS sim connects
//! to it as a "Linux / other" host so the full SSH client path +
//! Rust-side block state machine + renderer get exercised end-to-end
//! without needing an external SSH host.
//!
//! Usage:
//!   cargo run -p bedterm-mock-ssh -- [--port 2222]
//!
//! Then in the BedTerm sim, connect to:
//!   host:     127.0.0.1
//!   port:     2222
//!   user:     anything
//!   password: anything
//!
//! The server hands one canned scenario per shell session and closes.
//! Restart the connection in the app to replay.

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use russh::server::{Auth, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, CryptoVec, MethodSet};
use russh_keys::key::KeyPair;
use std::sync::Arc;
use std::time::Duration;
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

    let mut srv = MockServer;
    srv.run_on_address(config, addr).await?;
    Ok(())
}

#[derive(Clone)]
struct MockServer;

impl Server for MockServer {
    type Handler = MockHandler;
    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> MockHandler {
        MockHandler { script_done: false }
    }
}

struct MockHandler {
    /// One script per shell channel. After the script runs, the
    /// channel sits idle until the client closes.
    script_done: bool,
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
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.script_done {
            return Ok(());
        }
        self.script_done = true;
        let handle = session.handle();
        // Spawn the scripted output so we don't block russh's read loop.
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
        _data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Discard client-typed input — this mock doesn't run a real shell.
        // The block view only needs OSC 133-framed server output to
        // populate; user keypresses are unused here.
        Ok(())
    }
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
