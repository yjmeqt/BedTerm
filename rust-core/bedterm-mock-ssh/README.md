# bedterm-mock-ssh

Local-loopback SSH server for BedTerm sim testing. Binds **only** to
`127.0.0.1:2222` and accepts any username + password. Two modes:

| Mode | Flag | What it does |
|---|---|---|
| **Script** (default) | _none_ | Streams a canned OSC 133-framed transcript (`ls`, `echo <CJK+emoji>`, `false`, running prompt) and ignores client input. Exercises the renderer + block-state machine without needing a real shell. |
| **Shell bridge** | `--shell` | Spawns `$SHELL -l` (falls back to `/bin/zsh`) in a real PTY and proxies bytes both ways. Gives you an interactive shell as your current macOS user, no password, no `sshd`. |

Security: loopback-only is the moat. Do **not** change the bind address
to `0.0.0.0` or your LAN IP — that would expose a passwordless shell to
anyone on the network.

## Running

From the workspace root (`rust-core/`):

```sh
# Script mode — renderer/block-view fixture
cargo run -p bedterm-mock-ssh --release

# Real PTY bridged to your zsh — interactive testing
cargo run -p bedterm-mock-ssh --release -- --shell

# Different port
cargo run -p bedterm-mock-ssh --release -- --shell --port 2223
```

The server prints which mode it's in and the listen port on startup.

## Connecting from the iOS simulator

The sim shares the Mac's loopback, so `127.0.0.1:2222` works as-is. Two
shortcuts in DEBUG builds:

1. **Hosts list → "Mock SSH (loopback)"** — top row above your saved
   hosts. Routes through the real `CitadelSSHClient` with throwaway
   credentials (`user=test`, `password=x`). Use this for end-to-end
   testing of the SSH client path.
2. **`AppRoute.debugTerminal(.mockSSH)`** — the underlying route, also
   reachable programmatically.

Either entry point gives you the same session: SSH client → mock server
→ (canned script | real PTY).

## Typical workflows

**Renderer / block view smoke test** — script mode is enough:

```sh
cargo run -p bedterm-mock-ssh --release
# Sim → "Mock SSH (loopback)" → observe the three blocks animate in.
```

**Interactive feature testing** (composer, key bar, alt-screen, etc.) —
use `--shell` so you can actually type commands:

```sh
cargo run -p bedterm-mock-ssh --release -- --shell
# Sim → "Mock SSH (loopback)" → `vim`, `htop`, `claude`, whatever.
```

The shell starts in your `$HOME` with `TERM=xterm-256color` and
`LANG=en_US.UTF-8`. Window-change events from the client resize the
PTY, so `vim` etc. reflow correctly.

## Lifecycle

- Each connection gets a fresh ed25519 host key — fingerprints rotate
  every server restart on purpose, so the iOS keystore doesn't pin a
  stale key during development.
- In `--shell` mode the bridge thread tears down on child exit
  (`exit` / Ctrl-D in the shell), client `channel_close`, or process
  kill.
- In script mode one transcript replays per shell channel, then the
  channel goes idle until the client disconnects.
