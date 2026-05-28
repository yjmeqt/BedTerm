# bedterm-mock-ssh

Local-loopback SSH server for BedTerm sim testing. Binds **only** to
`127.0.0.1:2222`, accepts any username + password, and bridges every
session to a real PTY running `$SHELL -l` (falling back to `/bin/zsh`).
You get an interactive shell as your current macOS user, no password,
no `sshd`.

Security: loopback-only is the moat. Do **not** change the bind address
to `0.0.0.0` or your LAN IP — that would expose a passwordless shell to
anyone on the network.

## Running

From the workspace root (`rust-core/`):

```sh
cargo run -p bedterm-mock-ssh --release

# Different port
cargo run -p bedterm-mock-ssh --release -- --port 2223
```

The server prints the listen port and the bridged shell on startup.

## Connecting from the iOS simulator

The sim shares the Mac's loopback, so `127.0.0.1:2222` works as-is. In
BedTerm, add a regular host entry pointing at `127.0.0.1:2222` with any
username + password (e.g. `user=test`, `password=x`); it routes through
the real `RusshSSHClient` for end-to-end testing of the SSH client
path.

## Blocks during the session

The session runs your real shell, so blocks only appear if the BedTerm
shell-integration script has been sourced. Toggle **Settings → "Install
shell integration on connect"** (DEBUG builds) and BedTerm pushes
`bedterm-integration.sh` into the PTY at session start; subsequent
commands appear as Warp-style blocks in the BedTerm UI.
