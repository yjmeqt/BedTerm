//! In-process mock SSH client for UI tests.
//!
//! [`MockSshClient`] fulfils the [`SshClient`] trait with scripted, deterministic
//! behaviour — no real network connection required. It is wired in by
//! [`bt_terminal_session_install_mock`](crate::terminal_session) when the host app
//! is launched with `-uitest-stubSSH <script>`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Mutex as TokioMutex, Notify};

use crate::ssh_client::{PtyDimensions, SshClient, SshConnectionRequest, SshError};

// ---------------------------------------------------------------------------
// MockSshClient
// ---------------------------------------------------------------------------

/// In-process SSH client that returns scripted data without touching the
/// network.
///
/// Construct via [`MockSshClient::from_script`] with one of the named scripts
/// that mirror `UITestSupport.swift`:
///
/// | Script      | Behaviour                                                  |
/// |-------------|-------------------------------------------------------------|
/// | `"hello"`   | Emits `"Hello, world!\r\n"` then blocks until closed.      |
/// | `"ansiColors"` | Emits an ANSI-colour sequence then blocks until closed. |
/// | `"prompt"`  | Emits `"bedterm$ "` then blocks until closed.             |
/// | `"echo"`    | Emits `"bedterm$ "` then echoes each write back in magenta.|
///
/// Any other string is treated the same as `"hello"`.
pub struct MockSshClient {
    /// Pre-buffered output chunks drained on the first `read()` calls.
    output_buf: VecDeque<Vec<u8>>,
    /// When `true`, `write()` pushes data back into `echo_buf` for `read()`.
    echo_mode: bool,
    /// Shared queue between `write()` (producer) and `read()` (consumer).
    echo_buf: Arc<TokioMutex<VecDeque<Vec<u8>>>>,
    /// Notified by `close()` so a blocking `read()` can return EOF.
    close_notify: Arc<Notify>,
}

impl MockSshClient {
    /// Construct from a named script. See the type-level documentation for
    /// the mapping from script name to scripted behaviour.
    pub fn from_script(script: &str) -> Self {
        let initial: Vec<u8> = match script {
            "ansiColors" => {
                b"\x1B[31mred\x1B[0m \x1B[32mgreen\x1B[0m \x1B[34mblue\x1B[0m\r\n".to_vec()
            }
            "prompt" => b"bedterm$ ".to_vec(),
            "echo" => b"bedterm$ ".to_vec(),
            _ => b"Hello, world!\r\n".to_vec(), // "hello" + default
        };
        let echo_mode = script == "echo";
        MockSshClient {
            output_buf: VecDeque::from([initial]),
            echo_mode,
            echo_buf: Arc::new(TokioMutex::new(VecDeque::new())),
            close_notify: Arc::new(Notify::new()),
        }
    }
}

#[async_trait]
impl SshClient for MockSshClient {
    async fn connect(_request: SshConnectionRequest) -> Result<Self, SshError> {
        // `connect` is never called for the mock path — the session is
        // constructed directly via `from_script` in the FFI layer.
        Err(SshError::Other(
            "MockSshClient::connect is not supported — construct via from_script".into(),
        ))
    }

    async fn open_shell(&mut self, _dimensions: PtyDimensions) -> Result<(), SshError> {
        // Nothing to do: the scripted output is already staged.
        Ok(())
    }

    async fn read(&mut self) -> Result<Vec<u8>, SshError> {
        // Drain pre-buffered chunks first.
        if let Some(chunk) = self.output_buf.pop_front() {
            return Ok(chunk);
        }

        if self.echo_mode {
            // In echo mode: wait for a write()-fed echo chunk or close signal.
            loop {
                let notified = self.close_notify.notified();
                tokio::pin!(notified);
                tokio::select! {
                    _ = &mut notified => return Ok(vec![]),  // EOF on close
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {
                        let mut buf = self.echo_buf.lock().await;
                        if let Some(chunk) = buf.pop_front() {
                            return Ok(chunk);
                        }
                    }
                }
            }
        } else {
            // Non-echo: block until close is signalled, then return EOF.
            self.close_notify.notified().await;
            Ok(vec![])
        }
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), SshError> {
        if self.echo_mode && !data.is_empty() {
            // Echo: wrap in bright-magenta SGR, mirroring MockSSHClient.swift.
            let mut framed: Vec<u8> = b"\x1B[95m".to_vec();
            for &byte in data {
                // Print printable ASCII verbatim; replace others with '#'.
                if byte >= 0x20 && byte <= 0x7E {
                    framed.push(byte);
                } else {
                    framed.push(b'#');
                }
            }
            framed.extend_from_slice(b"\x1B[0m");
            self.echo_buf.lock().await.push_back(framed);
        }
        Ok(())
    }

    async fn resize(&mut self, _dimensions: PtyDimensions) -> Result<(), SshError> {
        Ok(())
    }

    async fn close(self) -> Result<(), SshError> {
        self.close_notify.notify_one();
        Ok(())
    }
}
