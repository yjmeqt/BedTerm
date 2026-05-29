# BedTerm

iOS terminal for AI-assisted coding. SSH into a dev box and run Claude Code / Codex CLI from iPhone.

## Development

Worktree-based branching model. Each feature gets its own git worktree.

### Skills

| Skill | What it does |
|-------|--------------|
| `/prd` | Load the product requirements document |
| `/xc-dev` | iOS build, test, run, simulator management within a worktree |

### Workflow

Worktrees live at `.worktrees/`.

```
1. git worktree add .worktrees/<feature-name> -b feature/<feature-name>
2. Build & iterate with /xc-dev (tasks defined in .xc-dev/tasks.toml)
3. Commit & merge back to main
```

### Target

iOS 26

## Build & test

```sh
brew install mint
mint bootstrap

# Rust toolchain — the terminal core lives in `rust-core/` and is packaged
# into a SwiftPM binary target. rustup picks up the channel pinned in
# rust-core/rust-toolchain.toml automatically.
rustup show
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin
```

Build and test go through `xc-dev` (tasks defined in `.xc-dev/tasks.toml`):

```sh
xc-dev build   # build for the configured simulator
xc-dev test    # run BedTermKitTests
xc-dev run     # launch the app in the simulator
```

The `BedTerm` scheme has a build pre-action that runs
`scripts/build-rust-xcframework.sh` automatically, so every build keeps
`BedTermKit/BinaryFrameworks/BedTermIOS.xcframework` in sync with
`rust-core/`. The script is idempotent — a no-op build skips the rebuild.

## Mock SSH for sim testing

`rust-core/bedterm-mock-ssh` is a loopback SSH server (binds only to
`127.0.0.1:2222`, accepts any user + password) so you can test the full
client → block-view path without a remote host. Two modes:

```sh
# Script mode — canned OSC 133 transcript, renderer/block-state fixture
cargo run -p bedterm-mock-ssh --release --manifest-path rust-core/Cargo.toml

# Shell mode — bridges to `$SHELL -l` in a real PTY (interactive zsh as you)
cargo run -p bedterm-mock-ssh --release --manifest-path rust-core/Cargo.toml -- --shell
```

Add a host pointing at `127.0.0.1:2222` (any username + password) to
exercise the real `CitadelSSHClient` against this server. See
`rust-core/bedterm-mock-ssh/README.md` for the full breakdown.

## Lint

```sh
xcrun swift-format lint -r --strict BedTerm BedTermKit/Sources BedTermKit/Tests
```

`swift-format` ships with Xcode 26 — no install needed.

