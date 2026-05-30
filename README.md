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

## Lint

```sh
xcrun swift-format lint -r --strict BedTerm BedTermKit/Sources BedTermKit/Tests
```

`swift-format` ships with Xcode 26 — no install needed.

